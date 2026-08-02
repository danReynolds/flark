//! Feature-gated ownership seam between the exact parser and candidate host.
//!
//! `flark-parser` is the only workspace consumer. The facade keeps arena IDs,
//! build journals, manifest internals, and host roots inside `flark-engine`.
//! This is a trusted workspace boundary, not a capability sandbox: its sole
//! consumer must preserve the documented single-transaction ownership rules.

use std::fmt;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;

use crate::block_quote_projection::PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES;
use crate::block_sequence::{
    persistent_m11_block_locate_byte, persistent_m11_block_locate_point,
    persistent_m11_block_visit_entries, plan_persistent_m11_block_semantic_splice,
    splice_persistent_m11_block_sequence_atomic, M11BlockSequenceVisitControl,
    M11BlockSequenceVisitDisposition, M11BlockSequenceVisitEntry, M11BlockSequenceVisitStart,
};
use crate::candidate_manifest::{
    decode_manifest_descriptor, manifest_digest256, persistent_block_manifest_roles,
    persistent_recursive_green_manifest_role, persistent_reference_manifest_root, role_index,
    CandidateAuthority, CandidateManifestAssembler, CandidateRole, CanonicalRoleInputs,
    ManifestError, ManifestPoll, PublishedManifest, StrongIdentity,
};
use crate::document::{DocumentRuntime, DocumentRuntimeError, PersistentSourceFactsDeltaWitness};
use crate::host_store::{
    classify_snapshot_frame, CandidateHostError, CandidateHostInstallPoll, CandidateHostLimits,
    CandidateHostStore, CandidateSnapshotEncodePoll, CandidateSnapshotEncoder,
    CandidateSnapshotEncoderState, InstalledCandidateSnapshot, SnapshotFrameKind,
    M11_MAXIMUM_SNAPSHOT_CHILDREN, M11_MAXIMUM_SNAPSHOT_FRAME_BYTES, SNAPSHOT_CHILD_ORDINAL_BYTES,
    SNAPSHOT_NODE_HEADER_BYTES,
};
use crate::indented_code_projection::PERSISTENT_INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES;
use crate::inline_overlay::{
    M11InlineOverlayBase, M11InlineOverlayBinding, M11InlineOverlayCanonicalLineEnding,
    M11InlineOverlayDisposition, M11InlineOverlayEnvelope, M11InlineOverlayOwner,
    M11InlineOverlayProjectionKind, M11InlineOverlaySnapshotEncodePoll,
    M11InlineOverlaySnapshotEncoder, M11InlineOverlayTransportError,
    M11_INLINE_OVERLAY_ENVELOPE_BYTES,
};
use crate::inline_projection::PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES;
use crate::m11_host::M11_CANDIDATE_ARENA_MAX_SLOTS;
use crate::recursive_green::plan_persistent_m11_recursive_green_semantic_splice;
use crate::reference_root::{
    AuthoritativeReferenceFact, AuthoritativeReferenceFactStart, ReferenceRootError,
    ReferenceRootLimits, ReferenceSourceRange, ReferenceWinnerIndex, ReferenceWinnerIndexBuilder,
    ReferenceWinnerIndexReclaimer, StreamedReferenceValueKind,
};
use crate::storage::{
    ArenaError, CommittedArenaRoot, ExternalPayloadReservation, PageArena, ARENA_PAGE_BYTES,
};
use crate::{CandidateGeneration, ParserProfileId, SourceFactsScanProfile, SourceVersion};

pub use crate::block_quote_projection::{
    BlockQuoteLineV1, M11BlockQuoteProjectionBuild, M11BlockQuoteProjectionBuildPoll,
    M11BlockQuoteProjectionBuildStatus, M11BlockQuoteProjectionCursor,
    M11BlockQuoteProjectionCursorPoll, M11BlockQuoteProjectionDescriptor,
    M11BlockQuoteProjectionError, M11BlockQuoteProjectionRoot, M11MarkedLineProjectionKind,
    BLOCK_QUOTE_LINES_PER_PAGE_MAX, BLOCK_QUOTE_LINE_FLAG_LAZY, BLOCK_QUOTE_LINE_FLAG_MARKED,
    BLOCK_QUOTE_LINE_V1_BYTES, BLOCK_QUOTE_WINDOW_MAX_BYTES,
};
pub use crate::block_sequence::{
    splice_m11_block_sequence_atomic, M11BlockRoleRecord, M11BlockSequenceBuild,
    M11BlockSequenceBuildPoll, M11BlockSequenceBuildReceipt, M11BlockSequenceBuildStatus,
    M11BlockSequenceEntry, M11BlockSequenceEntryKind, M11BlockSequenceError,
    M11BlockSequenceLocation, M11BlockSequencePoint, M11BlockSequenceQueryReceipt,
    M11BlockSequenceReclaimPoll, M11BlockSequenceRoot, M11BlockSequenceSpliceReceipt,
    M11BlockSequenceSpliceSelection, M11BlockUnsupportedReason,
    M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX, M11_BLOCK_SEQUENCE_MAX_POLL_TRANSITIONS,
    M11_BLOCK_SEQUENCE_MAX_ROLE_RECORD_BYTES,
};
pub use crate::indented_code_projection::{
    IndentedCodeLineV1, M11IndentedCodeProjectionBuild, M11IndentedCodeProjectionBuildPoll,
    M11IndentedCodeProjectionBuildStatus, M11IndentedCodeProjectionCursor,
    M11IndentedCodeProjectionCursorPoll, M11IndentedCodeProjectionDescriptor,
    M11IndentedCodeProjectionError, M11IndentedCodeProjectionRoot,
    INDENTED_CODE_LINES_PER_PAGE_MAX, INDENTED_CODE_LINE_FLAG_INTERNAL_BLANK,
    INDENTED_CODE_LINE_V1_BYTES, INDENTED_CODE_PROJECTION_FLAG_SYNTHETIC_FINAL_LF,
    INDENTED_CODE_WINDOW_MAX_BYTES,
};
pub use crate::inline_projection::{
    M11InlineLinkValue, M11InlineProjectionBuild, M11InlineProjectionBuildPoll,
    M11InlineProjectionBuildStatus, M11InlineProjectionCheckpointQuery,
    M11InlineProjectionCheckpointQueryPoll, M11InlineProjectionCursor,
    M11InlineProjectionCursorPoll, M11InlineProjectionDescriptor, M11InlineProjectionError,
    M11InlineProjectionFact, M11InlineProjectionKind, M11InlineProjectionRoot,
    M11_INLINE_CHARACTER_REFERENCE_SOURCE_MAX_BYTES, M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES,
    M11_INLINE_LINK_VALUES_MAX_ENTRIES, M11_INLINE_PROJECTION_FACTS_PER_PAGE_MAX,
    M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW,
    M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS,
    M11_INLINE_PROJECTION_FLAG_CODE_TRIM_ONE_SPACE,
};
pub use crate::parser_pages::{
    M11ParserPageBuild, M11ParserPageBuildPoll, M11ParserPageBuildReceipt,
    M11ParserPageBuildStatus, M11ParserPageCursor, M11ParserPageCursorPoll,
    M11ParserPageCursorReceipt, M11ParserPageDrain, M11ParserPageError, M11ParserPageReclaimPoll,
    M11ParserPageRecord, M11ParserPageRoot, M11ParserRangeCursor, M11ParserRangePoll,
    M11ParserRangeReceipt, M11ParserRangeStatus, M11ParserSourceRangeAuthority,
    M11_PARSER_PAGE_MAX_POLL_TRANSITIONS, M11_PARSER_PAGE_MAX_RECORD_BYTES,
    M11_PARSER_RANGE_MAX_POLL_BYTES,
};
pub use crate::parser_scratch::{
    M11ParserScratchAdmission, M11ParserScratchError, M11ParserScratchReleaseFailure,
};
pub use crate::recursive_green::*;
pub use crate::reference_journal::*;

/// Maximum canonical bytes in each parser role record.
///
/// Production `SourceFacts` bypasses this flat-record envelope by retaining
/// its measured root. A digest-only replacement remains invalid.
pub const M11_SINGLE_RECORD_MAX_BYTES: usize = ARENA_PAGE_BYTES - 108;
/// Maximum child fanout for flat roles and one snapshot node.
///
/// Document scale no longer maps to this constant: persistent measured roles
/// grow by tree height rather than one wrapper child per canonical page.
pub const M11_MAX_ROLE_RECORDS: usize = M11_MAXIMUM_SNAPSHOT_CHILDREN;
/// Maximum encoded bytes in one closed M1.1 snapshot frame, including the
/// fixed header, all serialized child ordinals, and the maximum arena payload.
pub const M11_MAX_SNAPSHOT_FRAME_BYTES: usize = M11_MAXIMUM_SNAPSHOT_FRAME_BYTES;
const M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug)]
enum ErrorInner {
    Invalid(&'static str),
    Arena(ArenaError),
    Document(DocumentRuntimeError),
    Manifest(ManifestError),
    Reference(ReferenceRootError),
    Host(CandidateHostError),
    BlockSequence(M11BlockSequenceError),
    RecursiveGreen(M11RecursiveGreenError),
    InlineOverlay(M11InlineOverlayTransportError),
}

/// Opaque failure from the workspace-internal publication seam.
#[derive(Debug)]
pub struct M11PublicationError(ErrorInner);

impl M11PublicationError {
    #[must_use]
    pub const fn zero_fuel() -> Self {
        Self(ErrorInner::Invalid(
            "publication poll requires nonzero fuel",
        ))
    }

    #[must_use]
    pub const fn invalid_state() -> Self {
        Self(ErrorInner::Invalid(
            "publication owner is not in the required state",
        ))
    }

    #[must_use]
    pub fn is_cross_authority(&self) -> bool {
        matches!(
            self.0,
            ErrorInner::Manifest(ManifestError::CrossAuthority)
                | ErrorInner::Host(CandidateHostError::CrossAuthority)
                | ErrorInner::BlockSequence(M11BlockSequenceError::SourceAuthorityMismatch)
        )
    }

    #[must_use]
    pub fn is_stale_candidate(&self) -> bool {
        matches!(self.0, ErrorInner::Host(CandidateHostError::StaleCandidate))
    }

    #[must_use]
    pub fn is_invalid_snapshot(&self) -> bool {
        matches!(
            self.0,
            ErrorInner::Host(
                CandidateHostError::InvalidFrame(_) | CandidateHostError::SourceFacts(_)
            )
        )
    }

    #[must_use]
    pub fn is_resource_limit(&self) -> bool {
        matches!(
            self.0,
            ErrorInner::Arena(
                ArenaError::CapacityExceeded
                    | ArenaError::PayloadTooLarge
                    | ArenaError::TooManyChildren
                    | ArenaError::PayloadBudgetExceeded
                    | ArenaError::BuildCapacityExceeded
                    | ArenaError::AllocationFailed
            ) | ErrorInner::Manifest(
                ManifestError::InvalidLimits
                    | ManifestError::CapacityPreflight
                    | ManifestError::Arena(
                        ArenaError::CapacityExceeded
                            | ArenaError::PayloadTooLarge
                            | ArenaError::TooManyChildren
                            | ArenaError::PayloadBudgetExceeded
                            | ArenaError::BuildCapacityExceeded
                            | ArenaError::AllocationFailed
                    )
            ) | ErrorInner::Host(CandidateHostError::AllocationFailed)
        )
    }
}

impl fmt::Display for M11PublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ErrorInner::Invalid(message) => formatter.write_str(message),
            ErrorInner::Arena(error) => error.fmt(formatter),
            ErrorInner::Document(error) => error.fmt(formatter),
            ErrorInner::Manifest(error) => error.fmt(formatter),
            ErrorInner::Reference(error) => error.fmt(formatter),
            ErrorInner::Host(error) => error.fmt(formatter),
            ErrorInner::BlockSequence(error) => error.fmt(formatter),
            ErrorInner::RecursiveGreen(error) => error.fmt(formatter),
            ErrorInner::InlineOverlay(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11PublicationError {}

impl From<ArenaError> for M11PublicationError {
    fn from(error: ArenaError) -> Self {
        Self(ErrorInner::Arena(error))
    }
}

impl From<DocumentRuntimeError> for M11PublicationError {
    fn from(error: DocumentRuntimeError) -> Self {
        Self(ErrorInner::Document(error))
    }
}

impl From<ManifestError> for M11PublicationError {
    fn from(error: ManifestError) -> Self {
        Self(ErrorInner::Manifest(error))
    }
}

impl From<ReferenceRootError> for M11PublicationError {
    fn from(error: ReferenceRootError) -> Self {
        Self(ErrorInner::Reference(error))
    }
}

impl From<CandidateHostError> for M11PublicationError {
    fn from(error: CandidateHostError) -> Self {
        Self(ErrorInner::Host(error))
    }
}

impl From<M11BlockSequenceError> for M11PublicationError {
    fn from(error: M11BlockSequenceError) -> Self {
        Self(ErrorInner::BlockSequence(error))
    }
}

impl From<M11RecursiveGreenError> for M11PublicationError {
    fn from(error: M11RecursiveGreenError) -> Self {
        Self(ErrorInner::RecursiveGreen(error))
    }
}

impl From<M11InlineOverlayTransportError> for M11PublicationError {
    fn from(error: M11InlineOverlayTransportError) -> Self {
        Self(ErrorInner::InlineOverlay(error))
    }
}

/// Canonical parser-owned records for the non-reference roles.
pub struct M11RoleRecords(CanonicalRoleInputs);

impl M11RoleRecords {
    /// Creates the legacy materialized role set used by isolated engine tests.
    /// Every payload contains a complete canonical record; production parser
    /// publication uses [`Self::persistent`].
    pub fn new(
        source_facts: impl IntoIterator<Item = Box<[u8]>>,
        green: impl Into<Box<[u8]>>,
        projection: impl Into<Box<[u8]>>,
    ) -> Result<Self, M11PublicationError> {
        let source_facts: Vec<Box<[u8]>> = source_facts.into_iter().collect();
        let green = green.into();
        let projection = projection.into();
        if source_facts.is_empty()
            || source_facts.len() > M11_MAX_ROLE_RECORDS
            || source_facts
                .iter()
                .any(|record| record.len() > M11_SINGLE_RECORD_MAX_BYTES)
            || green.len() > M11_SINGLE_RECORD_MAX_BYTES
            || projection.len() > M11_SINGLE_RECORD_MAX_BYTES
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "parser role exceeds the M1.1 record or fanout envelope",
            )));
        }
        Ok(Self(CanonicalRoleInputs::new(
            source_facts,
            green,
            projection,
        )))
    }

    /// Creates the materialized parser roles used beside the actor-owned
    /// persistent SourceFacts sequence.
    pub fn persistent(
        green: impl Into<Box<[u8]>>,
        projection: impl Into<Box<[u8]>>,
    ) -> Result<Self, M11PublicationError> {
        Self::persistent_projection_records(green, [projection.into()])
    }

    /// Creates one Green summary plus a bounded sequence of parser-authored
    /// Projection records.
    pub fn persistent_projection_records(
        green: impl Into<Box<[u8]>>,
        projection: impl IntoIterator<Item = Box<[u8]>>,
    ) -> Result<Self, M11PublicationError> {
        let green = green.into();
        let projection: Vec<Box<[u8]>> = projection.into_iter().collect();
        if green.len() > M11_SINGLE_RECORD_MAX_BYTES
            || projection.is_empty()
            || projection.len() > M11_MAX_ROLE_RECORDS
            || projection
                .iter()
                .any(|record| record.len() > M11_SINGLE_RECORD_MAX_BYTES)
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "parser role exceeds the M1.1 record envelope",
            )));
        }
        Ok(Self(CanonicalRoleInputs::persistent_projection_records(
            green, projection,
        )))
    }

    /// Creates bounded ordinary Projection records for publication beside a
    /// separately owned persistent recursive Green root.
    pub fn persistent_recursive_green_projection_records(
        projection: impl IntoIterator<Item = Box<[u8]>>,
    ) -> Result<Self, M11PublicationError> {
        let projection: Vec<Box<[u8]>> = projection.into_iter().collect();
        if projection.is_empty()
            || projection.len() > M11_MAX_ROLE_RECORDS
            || projection
                .iter()
                .any(|record| record.len() > M11_SINGLE_RECORD_MAX_BYTES)
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "parser Projection exceeds the M1.1 record envelope",
            )));
        }
        Ok(Self(
            CanonicalRoleInputs::persistent_recursive_green_projection_records(projection),
        ))
    }
}

/// Exact byte and UTF-16 range for one parser-authenticated source cut.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11ReferenceRange {
    bytes: Range<u64>,
    utf16: Range<u64>,
}

impl M11ReferenceRange {
    #[must_use]
    pub const fn new(bytes: Range<u64>, utf16: Range<u64>) -> Self {
        Self { bytes, utf16 }
    }
}

/// One exact, already-normalized and cooked reference-definition record.
pub struct M11ReferenceRecord {
    source: M11ReferenceRange,
    label_source: M11ReferenceRange,
    destination_source: M11ReferenceRange,
    title_source: Option<M11ReferenceRange>,
    normalized_label: Box<[u8]>,
    cooked_destination: Box<[u8]>,
    cooked_title: Option<Box<[u8]>>,
}

impl M11ReferenceRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        source: M11ReferenceRange,
        label_source: M11ReferenceRange,
        destination_source: M11ReferenceRange,
        title_source: Option<M11ReferenceRange>,
        normalized_label: impl Into<Box<[u8]>>,
        cooked_destination: impl Into<Box<[u8]>>,
        cooked_title: Option<Box<[u8]>>,
    ) -> Self {
        Self {
            source,
            label_source,
            destination_source,
            title_source,
            normalized_label: normalized_label.into(),
            cooked_destination: cooked_destination.into(),
            cooked_title,
        }
    }
}

/// Exact reference metadata whose cooked values will be streamed separately.
pub struct M11ReferenceRecordStart {
    source: M11ReferenceRange,
    label_source: M11ReferenceRange,
    destination_source: M11ReferenceRange,
    title_source: Option<M11ReferenceRange>,
    normalized_label: Box<[u8]>,
    destination_len: usize,
    title_len: Option<usize>,
}

impl M11ReferenceRecordStart {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        source: M11ReferenceRange,
        label_source: M11ReferenceRange,
        destination_source: M11ReferenceRange,
        title_source: Option<M11ReferenceRange>,
        normalized_label: impl Into<Box<[u8]>>,
        destination_len: usize,
        title_len: Option<usize>,
    ) -> Self {
        Self {
            source,
            label_source,
            destination_source,
            title_source,
            normalized_label: normalized_label.into(),
            destination_len,
            title_len,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11ReferenceValueKind {
    Destination,
    Title,
}

/// Exact immutable-tree work performed before persistent SourceFacts
/// publication begins.
///
/// This receipt deliberately excludes later snapshot traversal. It makes the
/// root-retain seam auditable: setup may inspect the retained root header and
/// fixed payload, but cannot walk or materialize its leaf sequence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11PersistentSourceFactsSetupReceipt {
    node_headers_decoded: u64,
    summary_combinations: u64,
    payload_bytes_inspected: u64,
    semantic_items_hashed: u64,
}

impl M11PersistentSourceFactsSetupReceipt {
    #[must_use]
    pub const fn node_headers_decoded(self) -> u64 {
        self.node_headers_decoded
    }

    #[must_use]
    pub const fn summary_combinations(self) -> u64 {
        self.summary_combinations
    }

    #[must_use]
    pub const fn payload_bytes_inspected(self) -> u64 {
        self.payload_bytes_inspected
    }

    #[must_use]
    pub const fn semantic_items_hashed(self) -> u64 {
        self.semantic_items_hashed
    }
}

/// Fuelled construction of one five-role candidate in its producer arena.
pub struct M11CandidateBuild {
    runtime_identity: StrongIdentity,
    assembler: Option<CandidateManifestAssembler>,
    publication: Option<crate::candidate_manifest::PublishedManifest>,
    persistent_source_facts_setup: Option<M11PersistentSourceFactsSetupReceipt>,
}

impl Drop for M11CandidateBuild {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            debug_assert!(
                self.assembler.is_none() && self.publication.is_none(),
                "M1.1 candidate build must abort-drain or transfer its publication"
            );
        }
    }
}

impl M11CandidateBuild {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        source: SourceVersion,
        parse_generation: u64,
        syntax_profile: u32,
        records: M11RoleRecords,
    ) -> Result<Self, M11PublicationError> {
        let parse_generation = CandidateGeneration::from_wire(parse_generation).ok_or(
            M11PublicationError(ErrorInner::Invalid("parse generation must be nonzero")),
        )?;
        let authority = CandidateAuthority::new(
            StrongIdentity::new(document)?,
            StrongIdentity::new(publication)?,
            source,
            parse_generation,
            syntax_profile,
        )?;
        let arena_limits = runtime.producer_arena().limits();
        if arena_limits.max_slots != M11_CANDIDATE_ARENA_MAX_SLOTS
            || arena_limits.max_live_payload_bytes != M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES
            || arena_limits.max_children_per_node != M11_MAX_ROLE_RECORDS
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "document runtime does not use the M1.1 producer arena envelope",
            )));
        }
        let limits = ReferenceRootLimits {
            arena: arena_limits,
            ..ReferenceRootLimits::default()
        };
        let assembler = CandidateManifestAssembler::new(
            runtime.producer_arena_mut(),
            authority,
            limits,
            records.0,
        )?;
        Ok(Self {
            runtime_identity: runtime.producer_identity(),
            assembler: Some(assembler),
            publication: None,
            persistent_source_facts_setup: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_persistent_source_facts(
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        source: SourceVersion,
        parse_generation: u64,
        syntax_profile: u32,
        source_facts_profile: SourceFactsScanProfile,
        records: M11RoleRecords,
    ) -> Result<Self, M11PublicationError> {
        let parse_generation = CandidateGeneration::from_wire(parse_generation).ok_or(
            M11PublicationError(ErrorInner::Invalid("parse generation must be nonzero")),
        )?;
        let parser_profile = ParserProfileId::new(u64::from(syntax_profile)).ok_or(
            M11PublicationError(ErrorInner::Invalid("syntax profile must be nonzero")),
        )?;
        let authority = CandidateAuthority::new(
            StrongIdentity::new(document)?,
            StrongIdentity::new(publication)?,
            source,
            parse_generation,
            syntax_profile,
        )?;
        let arena_limits = runtime.producer_arena().limits();
        if arena_limits.max_slots != M11_CANDIDATE_ARENA_MAX_SLOTS
            || arena_limits.max_live_payload_bytes != M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES
            || arena_limits.max_children_per_node != M11_MAX_ROLE_RECORDS
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "document runtime does not use the M1.1 producer arena envelope",
            )));
        }
        let limits = ReferenceRootLimits {
            arena: arena_limits,
            ..ReferenceRootLimits::default()
        };
        let runtime_identity = runtime.producer_identity();
        let (arena, persistent) = runtime.producer_arena_and_persistent_source_facts();
        let persistent = persistent.ok_or(M11PublicationError(ErrorInner::Invalid(
            "document runtime has no current persistent SourceFacts authority",
        )))?;
        let assembler = CandidateManifestAssembler::new_with_persistent_source_facts(
            arena,
            persistent,
            authority,
            source,
            parser_profile,
            source_facts_profile,
            limits,
            records.0,
        )?;
        let inspection = assembler
            .persistent_source_facts_setup()
            .expect("persistent constructor records its bounded setup");
        Ok(Self {
            runtime_identity,
            assembler: Some(assembler),
            publication: None,
            persistent_source_facts_setup: Some(M11PersistentSourceFactsSetupReceipt {
                node_headers_decoded: inspection.node_headers_decoded,
                summary_combinations: inspection.summary_combinations,
                payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
                semantic_items_hashed: inspection.spec.spec_items_hashed,
            }),
        })
    }

    /// Starts a candidate whose Green and Projection roles share one
    /// persistent block root behind fresh authority-bound wrappers.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_persistent_source_facts_and_blocks(
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        source: SourceVersion,
        parse_generation: u64,
        syntax_profile: u32,
        source_facts_profile: SourceFactsScanProfile,
        blocks: &M11BlockSequenceRoot,
    ) -> Result<Self, M11PublicationError> {
        let parse_generation = CandidateGeneration::from_wire(parse_generation).ok_or(
            M11PublicationError(ErrorInner::Invalid("parse generation must be nonzero")),
        )?;
        let parser_profile = ParserProfileId::new(u64::from(syntax_profile)).ok_or(
            M11PublicationError(ErrorInner::Invalid("syntax profile must be nonzero")),
        )?;
        let authority = CandidateAuthority::new(
            StrongIdentity::new(document)?,
            StrongIdentity::new(publication)?,
            source,
            parse_generation,
            syntax_profile,
        )?;
        let arena_limits = runtime.producer_arena().limits();
        if arena_limits.max_slots != M11_CANDIDATE_ARENA_MAX_SLOTS
            || arena_limits.max_live_payload_bytes != M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES
            || arena_limits.max_children_per_node != M11_MAX_ROLE_RECORDS
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "document runtime does not use the M1.1 producer arena envelope",
            )));
        }
        let limits = ReferenceRootLimits {
            arena: arena_limits,
            ..ReferenceRootLimits::default()
        };
        let runtime_identity = runtime.producer_identity();
        let (arena, persistent) = runtime.producer_arena_and_persistent_source_facts();
        let persistent = persistent.ok_or(M11PublicationError(ErrorInner::Invalid(
            "document runtime has no current persistent SourceFacts authority",
        )))?;
        let assembler = CandidateManifestAssembler::new_with_persistent_source_facts_and_blocks(
            arena,
            persistent,
            blocks,
            runtime_identity,
            authority,
            source,
            parser_profile,
            source_facts_profile,
            limits,
        )?;
        let inspection = assembler
            .persistent_source_facts_setup()
            .expect("persistent constructor records its bounded setup");
        Ok(Self {
            runtime_identity,
            assembler: Some(assembler),
            publication: None,
            persistent_source_facts_setup: Some(M11PersistentSourceFactsSetupReceipt {
                node_headers_decoded: inspection.node_headers_decoded,
                summary_combinations: inspection.summary_combinations,
                payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
                semantic_items_hashed: inspection.spec.spec_items_hashed,
            }),
        })
    }

    /// Starts a candidate whose Green role adopts one persistent recursive
    /// Green root and whose Projection role remains ordinary bounded records.
    ///
    /// `records` must come from
    /// [`M11RoleRecords::persistent_recursive_green_projection_records`], so
    /// no materialized Green record is accepted or silently discarded.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_persistent_source_facts_and_recursive_green(
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        source: SourceVersion,
        parse_generation: u64,
        syntax_profile: u32,
        source_facts_profile: SourceFactsScanProfile,
        records: M11RoleRecords,
        recursive_green: &M11RecursiveGreenRoot,
    ) -> Result<Self, M11PublicationError> {
        let parse_generation = CandidateGeneration::from_wire(parse_generation).ok_or(
            M11PublicationError(ErrorInner::Invalid("parse generation must be nonzero")),
        )?;
        let parser_profile = ParserProfileId::new(u64::from(syntax_profile)).ok_or(
            M11PublicationError(ErrorInner::Invalid("syntax profile must be nonzero")),
        )?;
        let authority = CandidateAuthority::new(
            StrongIdentity::new(document)?,
            StrongIdentity::new(publication)?,
            source,
            parse_generation,
            syntax_profile,
        )?;
        let arena_limits = runtime.producer_arena().limits();
        if arena_limits.max_slots != M11_CANDIDATE_ARENA_MAX_SLOTS
            || arena_limits.max_live_payload_bytes != M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES
            || arena_limits.max_children_per_node != M11_MAX_ROLE_RECORDS
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "document runtime does not use the M1.1 producer arena envelope",
            )));
        }
        let limits = ReferenceRootLimits {
            arena: arena_limits,
            ..ReferenceRootLimits::default()
        };
        let runtime_identity = runtime.producer_identity();
        let (arena, persistent) = runtime.producer_arena_and_persistent_source_facts();
        let persistent = persistent.ok_or(M11PublicationError(ErrorInner::Invalid(
            "document runtime has no current persistent SourceFacts authority",
        )))?;
        let assembler =
            CandidateManifestAssembler::new_with_persistent_source_facts_and_recursive_green(
                arena,
                persistent,
                recursive_green,
                runtime_identity,
                authority,
                source,
                parser_profile,
                source_facts_profile,
                limits,
                records.0,
            )?;
        let inspection = assembler
            .persistent_source_facts_setup()
            .expect("persistent constructor records its bounded setup");
        Ok(Self {
            runtime_identity,
            assembler: Some(assembler),
            publication: None,
            persistent_source_facts_setup: Some(M11PersistentSourceFactsSetupReceipt {
                node_headers_decoded: inspection.node_headers_decoded,
                summary_combinations: inspection.summary_combinations,
                payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
                semantic_items_hashed: inspection.spec.spec_items_hashed,
            }),
        })
    }

    /// Starts a cold recursive-Green candidate by retaining the parser
    /// session's already-committed References journal instead of rebuilding an
    /// equivalent second canonical reference tree.
    ///
    /// SourceFacts, recursive Green, and References are authenticated and
    /// retained in one failure-atomic candidate journal. The caller continues
    /// to own both `recursive_green` and `references`.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_persistent_source_facts_recursive_green_and_reference_journal(
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        source: SourceVersion,
        parse_generation: u64,
        syntax_profile: u32,
        source_facts_profile: SourceFactsScanProfile,
        records: M11RoleRecords,
        recursive_green: &M11RecursiveGreenRoot,
        references: &M11ReferenceJournalRoot,
    ) -> Result<Self, M11PublicationError> {
        let parse_generation = CandidateGeneration::from_wire(parse_generation).ok_or(
            M11PublicationError(ErrorInner::Invalid("parse generation must be nonzero")),
        )?;
        let parser_profile = ParserProfileId::new(u64::from(syntax_profile)).ok_or(
            M11PublicationError(ErrorInner::Invalid("syntax profile must be nonzero")),
        )?;
        let authority = CandidateAuthority::new(
            StrongIdentity::new(document)?,
            StrongIdentity::new(publication)?,
            source,
            parse_generation,
            syntax_profile,
        )?;
        let arena_limits = runtime.producer_arena().limits();
        if arena_limits.max_slots != M11_CANDIDATE_ARENA_MAX_SLOTS
            || arena_limits.max_live_payload_bytes != M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES
            || arena_limits.max_children_per_node != M11_MAX_ROLE_RECORDS
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "document runtime does not use the M1.1 producer arena envelope",
            )));
        }
        let limits = ReferenceRootLimits {
            arena: arena_limits,
            ..ReferenceRootLimits::default()
        };
        let runtime_identity = runtime.producer_identity();
        let (arena, persistent) = runtime.producer_arena_and_persistent_source_facts();
        let persistent = persistent.ok_or(M11PublicationError(ErrorInner::Invalid(
            "document runtime has no current persistent SourceFacts authority",
        )))?;
        let assembler = CandidateManifestAssembler::
            new_with_persistent_source_facts_recursive_green_and_reference_journal(
                arena,
                persistent,
                recursive_green,
                references,
                runtime_identity,
                authority,
                source,
                parser_profile,
                source_facts_profile,
                limits,
                records.0,
            )?;
        let inspection = assembler
            .persistent_source_facts_setup()
            .expect("persistent constructor records its bounded setup");
        Ok(Self {
            runtime_identity,
            assembler: Some(assembler),
            publication: None,
            persistent_source_facts_setup: Some(M11PersistentSourceFactsSetupReceipt {
                node_headers_decoded: inspection.node_headers_decoded,
                summary_combinations: inspection.summary_combinations,
                payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
                semantic_items_hashed: inspection.spec.spec_items_hashed,
            }),
        })
    }

    /// Starts an exact target whose Green role retains one recursive tree
    /// while References retain the canonical content of `base`.
    ///
    /// SourceFacts, recursive Green, and References are authenticated and
    /// retained in one failure-atomic target journal. The caller continues to
    /// own both `recursive_green` and `base`.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_persistent_source_facts_and_recursive_green_reusing_references(
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        source: SourceVersion,
        parse_generation: u64,
        syntax_profile: u32,
        source_facts_profile: SourceFactsScanProfile,
        records: M11RoleRecords,
        recursive_green: &M11RecursiveGreenRoot,
        base: &M11RetainedCandidatePublication,
    ) -> Result<Self, M11PublicationError> {
        validate_runtime(base.runtime_identity, runtime)?;
        let base_publication = base.publication()?;
        let parse_generation = CandidateGeneration::from_wire(parse_generation).ok_or(
            M11PublicationError(ErrorInner::Invalid("parse generation must be nonzero")),
        )?;
        let parser_profile = ParserProfileId::new(u64::from(syntax_profile)).ok_or(
            M11PublicationError(ErrorInner::Invalid("syntax profile must be nonzero")),
        )?;
        let authority = CandidateAuthority::new(
            StrongIdentity::new(document)?,
            StrongIdentity::new(publication)?,
            source,
            parse_generation,
            syntax_profile,
        )?;
        let arena_limits = runtime.producer_arena().limits();
        if arena_limits.max_slots != M11_CANDIDATE_ARENA_MAX_SLOTS
            || arena_limits.max_live_payload_bytes != M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES
            || arena_limits.max_children_per_node != M11_MAX_ROLE_RECORDS
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "document runtime does not use the M1.1 producer arena envelope",
            )));
        }
        let limits = ReferenceRootLimits {
            arena: arena_limits,
            ..ReferenceRootLimits::default()
        };
        let runtime_identity = runtime.producer_identity();
        let (arena, persistent) = runtime.producer_arena_and_persistent_source_facts();
        let persistent = persistent.ok_or(M11PublicationError(ErrorInner::Invalid(
            "document runtime has no current persistent SourceFacts authority",
        )))?;
        let assembler = CandidateManifestAssembler::
            new_with_persistent_source_facts_and_recursive_green_reusing_references(
                arena,
                persistent,
                recursive_green,
                runtime_identity,
                authority,
                source,
                parser_profile,
                source_facts_profile,
                limits,
                records.0,
                base_publication,
            )?;
        let inspection = assembler
            .persistent_source_facts_setup()
            .expect("persistent constructor records its bounded setup");
        Ok(Self {
            runtime_identity,
            assembler: Some(assembler),
            publication: None,
            persistent_source_facts_setup: Some(M11PersistentSourceFactsSetupReceipt {
                node_headers_decoded: inspection.node_headers_decoded,
                summary_combinations: inspection.summary_combinations,
                payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
                semantic_items_hashed: inspection.spec.spec_items_hashed,
            }),
        })
    }

    /// Starts a candidate that directly adopts one typed persistent inline
    /// Projection root beside the legacy bounded structural Projection
    /// records.
    ///
    /// The measured closure is retained into the candidate journal without
    /// replaying logical pages through `Box<[u8]>`. The caller continues to
    /// own `inline_projection` and must explicitly release that original
    /// committed capability after this constructor succeeds.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_persistent_source_facts_and_inline_projection(
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        source: SourceVersion,
        parse_generation: u64,
        syntax_profile: u32,
        source_facts_profile: SourceFactsScanProfile,
        records: M11RoleRecords,
        inline_projection: &M11InlineProjectionRoot,
    ) -> Result<Self, M11PublicationError> {
        let parse_generation = CandidateGeneration::from_wire(parse_generation).ok_or(
            M11PublicationError(ErrorInner::Invalid("parse generation must be nonzero")),
        )?;
        let parser_profile = ParserProfileId::new(u64::from(syntax_profile)).ok_or(
            M11PublicationError(ErrorInner::Invalid("syntax profile must be nonzero")),
        )?;
        let authority = CandidateAuthority::new(
            StrongIdentity::new(document)?,
            StrongIdentity::new(publication)?,
            source,
            parse_generation,
            syntax_profile,
        )?;
        let arena_limits = runtime.producer_arena().limits();
        if arena_limits.max_slots != M11_CANDIDATE_ARENA_MAX_SLOTS
            || arena_limits.max_live_payload_bytes != M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES
            || arena_limits.max_children_per_node != M11_MAX_ROLE_RECORDS
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "document runtime does not use the M1.1 producer arena envelope",
            )));
        }
        let limits = ReferenceRootLimits {
            arena: arena_limits,
            ..ReferenceRootLimits::default()
        };
        let runtime_identity = runtime.producer_identity();
        let (arena, persistent) = runtime.producer_arena_and_persistent_source_facts();
        let persistent = persistent.ok_or(M11PublicationError(ErrorInner::Invalid(
            "document runtime has no current persistent SourceFacts authority",
        )))?;
        let assembler =
            CandidateManifestAssembler::new_with_persistent_source_facts_and_inline_projection(
                arena,
                persistent,
                inline_projection,
                runtime_identity,
                authority,
                source,
                parser_profile,
                source_facts_profile,
                limits,
                records.0,
            )?;
        let inspection = assembler
            .persistent_source_facts_setup()
            .expect("persistent constructor records its bounded setup");
        Ok(Self {
            runtime_identity,
            assembler: Some(assembler),
            publication: None,
            persistent_source_facts_setup: Some(M11PersistentSourceFactsSetupReceipt {
                node_headers_decoded: inspection.node_headers_decoded,
                summary_combinations: inspection.summary_combinations,
                payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
                semantic_items_hashed: inspection.spec.spec_items_hashed,
            }),
        })
    }

    /// Starts a target candidate over the runtime's current persistent
    /// SourceFacts while retaining the canonical References root of `base`.
    ///
    /// The manifest assembler authenticates and retains the referenced
    /// canonical root under fresh target-authority wrappers. The caller keeps
    /// owning `base`; no base publication capability is consumed here.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_persistent_source_facts_reusing_references(
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        source: SourceVersion,
        parse_generation: u64,
        syntax_profile: u32,
        source_facts_profile: SourceFactsScanProfile,
        records: M11RoleRecords,
        base: &M11RetainedCandidatePublication,
    ) -> Result<Self, M11PublicationError> {
        validate_runtime(base.runtime_identity, runtime)?;
        let base_publication = base.publication()?;
        let parse_generation = CandidateGeneration::from_wire(parse_generation).ok_or(
            M11PublicationError(ErrorInner::Invalid("parse generation must be nonzero")),
        )?;
        let parser_profile = ParserProfileId::new(u64::from(syntax_profile)).ok_or(
            M11PublicationError(ErrorInner::Invalid("syntax profile must be nonzero")),
        )?;
        let authority = CandidateAuthority::new(
            StrongIdentity::new(document)?,
            StrongIdentity::new(publication)?,
            source,
            parse_generation,
            syntax_profile,
        )?;
        let arena_limits = runtime.producer_arena().limits();
        if arena_limits.max_slots != M11_CANDIDATE_ARENA_MAX_SLOTS
            || arena_limits.max_live_payload_bytes != M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES
            || arena_limits.max_children_per_node != M11_MAX_ROLE_RECORDS
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "document runtime does not use the M1.1 producer arena envelope",
            )));
        }
        let limits = ReferenceRootLimits {
            arena: arena_limits,
            ..ReferenceRootLimits::default()
        };
        let runtime_identity = runtime.producer_identity();
        let (arena, persistent) = runtime.producer_arena_and_persistent_source_facts();
        let persistent = persistent.ok_or(M11PublicationError(ErrorInner::Invalid(
            "document runtime has no current persistent SourceFacts authority",
        )))?;
        let assembler =
            CandidateManifestAssembler::new_with_persistent_source_facts_reusing_references(
                arena,
                persistent,
                authority,
                source,
                parser_profile,
                source_facts_profile,
                limits,
                records.0,
                base_publication,
            )?;
        let inspection = assembler
            .persistent_source_facts_setup()
            .expect("persistent constructor records its bounded setup");
        Ok(Self {
            runtime_identity,
            assembler: Some(assembler),
            publication: None,
            persistent_source_facts_setup: Some(M11PersistentSourceFactsSetupReceipt {
                node_headers_decoded: inspection.node_headers_decoded,
                summary_combinations: inspection.summary_combinations,
                payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
                semantic_items_hashed: inspection.spec.spec_items_hashed,
            }),
        })
    }

    /// Starts an exact target whose Green and Projection roles share one
    /// persistent block root while References retain the canonical content of
    /// `base`.
    ///
    /// SourceFacts, blocks, and References are authenticated and retained in
    /// one failure-atomic target journal. The caller continues to own both
    /// `blocks` and `base`.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_persistent_source_facts_and_blocks_reusing_references(
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        source: SourceVersion,
        parse_generation: u64,
        syntax_profile: u32,
        source_facts_profile: SourceFactsScanProfile,
        blocks: &M11BlockSequenceRoot,
        base: &M11RetainedCandidatePublication,
    ) -> Result<Self, M11PublicationError> {
        validate_runtime(base.runtime_identity, runtime)?;
        let base_publication = base.publication()?;
        let parse_generation = CandidateGeneration::from_wire(parse_generation).ok_or(
            M11PublicationError(ErrorInner::Invalid("parse generation must be nonzero")),
        )?;
        let parser_profile = ParserProfileId::new(u64::from(syntax_profile)).ok_or(
            M11PublicationError(ErrorInner::Invalid("syntax profile must be nonzero")),
        )?;
        let authority = CandidateAuthority::new(
            StrongIdentity::new(document)?,
            StrongIdentity::new(publication)?,
            source,
            parse_generation,
            syntax_profile,
        )?;
        let arena_limits = runtime.producer_arena().limits();
        if arena_limits.max_slots != M11_CANDIDATE_ARENA_MAX_SLOTS
            || arena_limits.max_live_payload_bytes != M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES
            || arena_limits.max_children_per_node != M11_MAX_ROLE_RECORDS
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "document runtime does not use the M1.1 producer arena envelope",
            )));
        }
        let limits = ReferenceRootLimits {
            arena: arena_limits,
            ..ReferenceRootLimits::default()
        };
        let runtime_identity = runtime.producer_identity();
        let (arena, persistent) = runtime.producer_arena_and_persistent_source_facts();
        let persistent = persistent.ok_or(M11PublicationError(ErrorInner::Invalid(
            "document runtime has no current persistent SourceFacts authority",
        )))?;
        let assembler = CandidateManifestAssembler::
            new_with_persistent_source_facts_and_blocks_reusing_references(
                arena,
                persistent,
                blocks,
                runtime_identity,
                authority,
                source,
                parser_profile,
                source_facts_profile,
                limits,
                base_publication,
            )?;
        let inspection = assembler
            .persistent_source_facts_setup()
            .expect("persistent constructor records its bounded setup");
        Ok(Self {
            runtime_identity,
            assembler: Some(assembler),
            publication: None,
            persistent_source_facts_setup: Some(M11PersistentSourceFactsSetupReceipt {
                node_headers_decoded: inspection.node_headers_decoded,
                summary_combinations: inspection.summary_combinations,
                payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
                semantic_items_hashed: inspection.spec.spec_items_hashed,
            }),
        })
    }

    /// Starts an exact-crop target that combines the runtime's current
    /// persistent SourceFacts, one typed persistent inline Projection root,
    /// and the canonical References root of `base`.
    ///
    /// The assembler authenticates all three roots and retains them in one
    /// fresh target journal. The caller continues to own both
    /// `inline_projection` and `base`; the original inline root must be
    /// explicitly released after this constructor succeeds.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_persistent_source_facts_and_inline_projection_reusing_references(
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        source: SourceVersion,
        parse_generation: u64,
        syntax_profile: u32,
        source_facts_profile: SourceFactsScanProfile,
        records: M11RoleRecords,
        inline_projection: &M11InlineProjectionRoot,
        base: &M11RetainedCandidatePublication,
    ) -> Result<Self, M11PublicationError> {
        validate_runtime(base.runtime_identity, runtime)?;
        let base_publication = base.publication()?;
        let parse_generation = CandidateGeneration::from_wire(parse_generation).ok_or(
            M11PublicationError(ErrorInner::Invalid("parse generation must be nonzero")),
        )?;
        let parser_profile = ParserProfileId::new(u64::from(syntax_profile)).ok_or(
            M11PublicationError(ErrorInner::Invalid("syntax profile must be nonzero")),
        )?;
        let authority = CandidateAuthority::new(
            StrongIdentity::new(document)?,
            StrongIdentity::new(publication)?,
            source,
            parse_generation,
            syntax_profile,
        )?;
        let arena_limits = runtime.producer_arena().limits();
        if arena_limits.max_slots != M11_CANDIDATE_ARENA_MAX_SLOTS
            || arena_limits.max_live_payload_bytes != M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES
            || arena_limits.max_children_per_node != M11_MAX_ROLE_RECORDS
        {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "document runtime does not use the M1.1 producer arena envelope",
            )));
        }
        let limits = ReferenceRootLimits {
            arena: arena_limits,
            ..ReferenceRootLimits::default()
        };
        let runtime_identity = runtime.producer_identity();
        let (arena, persistent) = runtime.producer_arena_and_persistent_source_facts();
        let persistent = persistent.ok_or(M11PublicationError(ErrorInner::Invalid(
            "document runtime has no current persistent SourceFacts authority",
        )))?;
        let assembler = CandidateManifestAssembler::
            new_with_persistent_source_facts_and_inline_projection_reusing_references(
                arena,
                persistent,
                inline_projection,
                runtime_identity,
                authority,
                source,
                parser_profile,
                source_facts_profile,
                limits,
                records.0,
                base_publication,
            )?;
        let inspection = assembler
            .persistent_source_facts_setup()
            .expect("persistent constructor records its bounded setup");
        Ok(Self {
            runtime_identity,
            assembler: Some(assembler),
            publication: None,
            persistent_source_facts_setup: Some(M11PersistentSourceFactsSetupReceipt {
                node_headers_decoded: inspection.node_headers_decoded,
                summary_combinations: inspection.summary_combinations,
                payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
                semantic_items_hashed: inspection.spec.spec_items_hashed,
            }),
        })
    }

    #[must_use]
    pub const fn persistent_source_facts_setup_receipt(
        &self,
    ) -> Option<M11PersistentSourceFactsSetupReceipt> {
        self.persistent_source_facts_setup
    }

    pub fn offer_reference(
        &mut self,
        runtime: &DocumentRuntime,
        record: M11ReferenceRecord,
    ) -> Result<(), M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        let assembler = self
            .assembler
            .as_mut()
            .ok_or(M11PublicationError(ErrorInner::Invalid(
                "candidate build is no longer accepting references",
            )))?;
        let range = |range: M11ReferenceRange| ReferenceSourceRange {
            bytes: range.bytes,
            utf16: range.utf16,
        };
        assembler.offer_reference(
            runtime.producer_arena(),
            AuthoritativeReferenceFact {
                authority: assembler.authority(),
                source: range(record.source),
                label_source: range(record.label_source),
                destination_source: range(record.destination_source),
                title_source: record.title_source.map(range),
                normalized_label: record.normalized_label,
                cooked_destination: record.cooked_destination,
                cooked_title: record.cooked_title,
                _not_sync: PhantomData,
            },
        )?;
        Ok(())
    }

    /// Begins one exact reference whose cooked values arrive incrementally.
    ///
    /// # Errors
    ///
    /// Returns a typed authority, range, capacity, or lifecycle failure.
    pub fn begin_reference_stream(
        &mut self,
        runtime: &DocumentRuntime,
        record: M11ReferenceRecordStart,
    ) -> Result<(), M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        let assembler = self
            .assembler
            .as_mut()
            .ok_or(M11PublicationError(ErrorInner::Invalid(
                "candidate build is no longer accepting references",
            )))?;
        let range = |range: M11ReferenceRange| ReferenceSourceRange {
            bytes: range.bytes,
            utf16: range.utf16,
        };
        assembler.begin_reference_stream(
            runtime.producer_arena(),
            AuthoritativeReferenceFactStart {
                authority: assembler.authority(),
                source: range(record.source),
                label_source: range(record.label_source),
                destination_source: range(record.destination_source),
                title_source: record.title_source.map(range),
                normalized_label: record.normalized_label,
                destination_len: record.destination_len,
                title_len: record.title_len,
                _not_sync: PhantomData,
            },
        )?;
        Ok(())
    }

    /// Returns the bounded bytes currently accepted for `kind`.
    ///
    /// # Errors
    ///
    /// Returns a typed publication state or reference-stream failure.
    pub fn reference_stream_capacity(
        &self,
        kind: M11ReferenceValueKind,
    ) -> Result<usize, M11PublicationError> {
        let kind = match kind {
            M11ReferenceValueKind::Destination => StreamedReferenceValueKind::Destination,
            M11ReferenceValueKind::Title => StreamedReferenceValueKind::Title,
        };
        self.assembler
            .as_ref()
            .ok_or(M11PublicationError(ErrorInner::Invalid(
                "candidate build is no longer accepting references",
            )))?
            .reference_stream_capacity(kind)
            .map_err(Into::into)
    }

    /// Copies a bounded prefix of cooked bytes into the active page buffer.
    ///
    /// # Errors
    ///
    /// Returns a typed publication state, value-order, or length failure.
    pub fn offer_reference_stream_bytes(
        &mut self,
        kind: M11ReferenceValueKind,
        bytes: &[u8],
    ) -> Result<usize, M11PublicationError> {
        let kind = match kind {
            M11ReferenceValueKind::Destination => StreamedReferenceValueKind::Destination,
            M11ReferenceValueKind::Title => StreamedReferenceValueKind::Title,
        };
        self.assembler
            .as_mut()
            .ok_or(M11PublicationError(ErrorInner::Invalid(
                "candidate build is no longer accepting references",
            )))?
            .offer_reference_stream_bytes(kind, bytes)
            .map_err(Into::into)
    }

    #[must_use]
    pub fn reference_stream_retained_bytes(&self) -> usize {
        self.assembler
            .as_ref()
            .map_or(0, |assembler| assembler.reference_stream_retained_bytes())
    }

    #[must_use]
    pub fn references_idle(&self) -> bool {
        self.assembler
            .as_ref()
            .is_some_and(CandidateManifestAssembler::references_idle)
    }

    pub fn finish_references(
        &mut self,
        runtime: &DocumentRuntime,
    ) -> Result<(), M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        self.assembler
            .as_mut()
            .ok_or(M11PublicationError(ErrorInner::Invalid(
                "candidate build is no longer active",
            )))?
            .finish_references(runtime.producer_arena())?;
        Ok(())
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11CandidateBuildPoll, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        let assembler = self
            .assembler
            .as_mut()
            .ok_or(M11PublicationError(ErrorInner::Invalid(
                "candidate build is no longer active",
            )))?;
        match assembler.poll(runtime.producer_arena_mut(), fuel)? {
            ManifestPoll::Pending { transitions } => {
                Ok(M11CandidateBuildPoll::Pending { transitions })
            }
            ManifestPoll::Published {
                transitions,
                publication,
            } => {
                self.publication = Some(publication);
                self.assembler.take();
                Ok(M11CandidateBuildPoll::Published { transitions })
            }
            ManifestPoll::Aborting => Err(M11PublicationError(ErrorInner::Invalid(
                "candidate build is aborting",
            ))),
        }
    }

    pub fn begin_abort(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        self.assembler
            .as_mut()
            .ok_or(M11PublicationError(ErrorInner::Invalid(
                "candidate build is no longer active",
            )))?
            .begin_abort(runtime.producer_arena_mut())?;
        Ok(())
    }

    pub fn poll_abort(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<bool, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        if self.assembler.is_none() || self.publication.is_some() {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "candidate build is not aborting",
            )));
        }
        if fuel == 0 {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "candidate abort requires nonzero fuel",
            )));
        }
        let complete = runtime.producer_arena_mut().poll_reclaim(fuel).complete;
        if complete {
            self.assembler.take();
        }
        Ok(complete)
    }

    pub fn into_publication(mut self) -> Result<M11CandidatePublication, M11PublicationError> {
        if self.assembler.is_some() {
            return Err(M11PublicationError(ErrorInner::Invalid(
                "candidate build has not published",
            )));
        }
        let publication =
            self.publication
                .take()
                .ok_or(M11PublicationError(ErrorInner::Invalid(
                    "candidate build has no publication",
                )))?;
        Ok(M11CandidatePublication {
            runtime_identity: self.runtime_identity,
            publication: Some(publication),
            closing_root: None,
            exact_block_splice: None,
            close_complete: false,
        })
    }
}

pub enum M11CandidateBuildPoll {
    Pending { transitions: usize },
    Published { transitions: usize },
}

/// One sealed producer publication. It must be explicitly fuel-closed.
pub struct M11CandidatePublication {
    runtime_identity: StrongIdentity,
    publication: Option<PublishedManifest>,
    closing_root: Option<CommittedArenaRoot>,
    exact_block_splice: Option<(
        M11BlockSequenceSpliceSelection,
        M11BlockSequenceSpliceReceipt,
    )>,
    close_complete: bool,
}

impl Drop for M11CandidatePublication {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            debug_assert!(
                self.close_complete && self.publication.is_none() && self.closing_root.is_none(),
                "M1.1 publication must transfer or fuel-close its runtime-owned root"
            );
        }
    }
}

/// Authenticated fixed descriptor available before one-pass snapshot output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11CandidateDescriptor {
    pub document: [u8; 16],
    pub publication: [u8; 16],
    pub source_root: u64,
    pub source_revision: u64,
    pub source_bytes: u64,
    pub source_utf16: u64,
    pub parse_generation: u64,
    pub syntax_profile: u32,
    pub canonical_record_count: u64,
    pub manifest_digest256: [u8; 32],
    pub maximum_snapshot_frames: u64,
    pub maximum_snapshot_encoded_bytes: u64,
}

impl M11CandidatePublication {
    /// Attaches the parser-selected block splice that constructed this exact
    /// target publication.
    ///
    /// The selection remains non-authoritative until exact-stream setup
    /// revalidates it against both the retained base and this sealed target.
    #[doc(hidden)]
    pub fn attach_exact_block_splice(
        &mut self,
        selection: M11BlockSequenceSpliceSelection,
        receipt: M11BlockSequenceSpliceReceipt,
    ) -> Result<(), M11PublicationError> {
        if self.publication.is_none()
            || self.closing_root.is_some()
            || self.exact_block_splice.is_some()
        {
            return Err(M11PublicationError::invalid_state());
        }
        self.exact_block_splice = Some((selection, receipt));
        Ok(())
    }

    /// Returns the parser-selected semantic block ranges, when local block
    /// splicing was sound for this exact target.
    #[must_use]
    pub fn exact_block_splice_selection(&self) -> Option<&M11BlockSequenceSpliceSelection> {
        self.exact_block_splice
            .as_ref()
            .map(|(selection, _)| selection)
    }

    /// Returns the producer work receipt for the local block splice.
    #[must_use]
    pub fn exact_block_splice_receipt(&self) -> Option<M11BlockSequenceSpliceReceipt> {
        self.exact_block_splice
            .as_ref()
            .map(|(_, receipt)| *receipt)
    }

    /// Reads the validated manifest descriptor without starting traversal.
    pub fn descriptor(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<M11CandidateDescriptor, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        descriptor_from_publication(runtime.producer_arena(), self.publication()?)
    }

    pub fn snapshot_encoder<'a>(
        &'a self,
        runtime: &'a DocumentRuntime,
    ) -> Result<M11SnapshotEncoder<'a>, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        Ok(M11SnapshotEncoder(CandidateSnapshotEncoder::new(
            runtime.producer_arena(),
            self.publication()?,
        )?))
    }

    /// Transfers the publication into an owned, fuel-driven snapshot stream.
    ///
    /// The stream retains only the sealed publication capability. Every arena
    /// read remains a short-lived borrow from its document runtime.
    pub fn into_snapshot_stream(
        self: Box<Self>,
        runtime: &DocumentRuntime,
    ) -> Result<M11OwnedSnapshotStream, M11SnapshotStreamStartFailure> {
        if let Err(error) = validate_runtime(self.runtime_identity, runtime) {
            return Err(M11SnapshotStreamStartFailure {
                error,
                publication: self,
            });
        }
        let (state, transferred_canonical_record_count) =
            match self.publication().and_then(|publication| {
                let state =
                    CandidateSnapshotEncoderState::new(runtime.producer_arena(), publication)?;
                let descriptor =
                    descriptor_from_publication(runtime.producer_arena(), publication)?;
                Ok((state, descriptor.canonical_record_count))
            }) {
                Ok(state) => state,
                Err(error) => {
                    return Err(M11SnapshotStreamStartFailure {
                        error,
                        publication: self,
                    })
                }
            };
        let mut publication_owner = self;
        let runtime_identity = publication_owner.runtime_identity;
        let publication = publication_owner.publication.take();
        let closing_root = publication_owner.closing_root.take();
        publication_owner.close_complete = true;
        Ok(M11OwnedSnapshotStream {
            runtime_identity,
            publication,
            closing_root,
            exact_base_publication: None,
            exact_base_closing_root: None,
            reference_winner: None,
            close_complete: false,
            traversal_complete: false,
            cancelled_for_exact_base_restore: false,
            transferred_canonical_record_count,
            state,
        })
    }

    /// Transfers a fresh target and one retained base into an exact-base
    /// SourceFacts delta stream.
    ///
    /// All publication and witness checks complete before the runtime consumes
    /// the one-use witness. If the witness itself is rejected as stale or
    /// foreign, the failure reports it as consumed so callers cannot silently
    /// downgrade the same candidate to a full snapshot.
    pub fn into_exact_base_snapshot_stream(
        self: Box<Self>,
        runtime: &mut DocumentRuntime,
        base: Box<M11RetainedCandidatePublication>,
        witness: Box<PersistentSourceFactsDeltaWitness>,
    ) -> Result<M11OwnedSnapshotStream, M11ExactBaseSnapshotStreamStartFailure> {
        self.into_exact_base_snapshot_stream_inner(runtime, base, witness, false, None)
    }

    /// Selects the parser-attached local block splice when one exists and
    /// otherwise preserves the SourceFacts-only exact-base transaction.
    ///
    /// Exact setup independently validates the semantic ranges against the
    /// retained base and sealed target before consuming the SourceFacts
    /// witness.
    pub fn into_exact_base_snapshot_stream_selecting_block_splice(
        self: Box<Self>,
        runtime: &mut DocumentRuntime,
        base: Box<M11RetainedCandidatePublication>,
        witness: Box<PersistentSourceFactsDeltaWitness>,
    ) -> Result<M11OwnedSnapshotStream, M11ExactBaseSnapshotStreamStartFailure> {
        self.into_exact_base_snapshot_stream_inner(runtime, base, witness, true, None)
    }

    /// Selects one parser-authenticated recursive-Green event splice for an
    /// exact-base transaction.
    ///
    /// Stream setup independently maps the semantic event ranges to complete
    /// packed leaves in both sealed publications before consuming the
    /// SourceFacts witness. Branch pages are reconstructed by the host rather
    /// than transferred.
    pub fn into_exact_base_snapshot_stream_selecting_recursive_green_splice(
        self: Box<Self>,
        runtime: &mut DocumentRuntime,
        base: Box<M11RetainedCandidatePublication>,
        witness: Box<PersistentSourceFactsDeltaWitness>,
        selection: M11RecursiveGreenStructuralSpliceSelection,
    ) -> Result<M11OwnedSnapshotStream, M11ExactBaseSnapshotStreamStartFailure> {
        self.into_exact_base_snapshot_stream_inner(runtime, base, witness, false, Some(selection))
    }

    fn into_exact_base_snapshot_stream_inner(
        self: Box<Self>,
        runtime: &mut DocumentRuntime,
        base: Box<M11RetainedCandidatePublication>,
        witness: Box<PersistentSourceFactsDeltaWitness>,
        select_block_splice: bool,
        recursive_green_selection: Option<M11RecursiveGreenStructuralSpliceSelection>,
    ) -> Result<M11OwnedSnapshotStream, M11ExactBaseSnapshotStreamStartFailure> {
        let setup = (|| {
            if select_block_splice && recursive_green_selection.is_some() {
                return Err(M11PublicationError::invalid_state());
            }
            validate_runtime(self.runtime_identity, runtime)?;
            validate_runtime(base.runtime_identity, runtime)?;
            let target_publication = self.publication()?;
            let base_publication = base.publication()?;
            validate_exact_base_witness(
                runtime.producer_arena(),
                base_publication,
                target_publication,
                &witness,
            )?;
            if let Some(winner) = base.reference_winner.as_ref() {
                let target_descriptor = decode_manifest_descriptor(
                    runtime.producer_arena(),
                    target_publication.root_id(),
                    target_publication.authority(),
                )?;
                let target_reference_root = persistent_reference_manifest_root(
                    runtime.producer_arena(),
                    &target_descriptor,
                    target_publication.authority(),
                )?;
                if winner.root != target_reference_root {
                    return Err(M11PublicationError::invalid_state());
                }
            }
            let block_selection = select_block_splice
                .then_some(self.exact_block_splice.as_ref())
                .flatten()
                .map(|(selection, _)| selection);
            let (state, transferred_canonical_record_count) = if let Some(selection) =
                recursive_green_selection
            {
                let transferred = exact_base_transferred_record_count_with_recursive_green_splice(
                    runtime.producer_arena(),
                    target_publication,
                    witness.target_page_range(),
                    selection,
                )?;
                let state = CandidateSnapshotEncoderState::
                    new_exact_base_delta_with_recursive_green_splice(
                        runtime.producer_arena(),
                        base_publication,
                        target_publication,
                        witness.base_page_range().clone(),
                        witness.target_page_range().clone(),
                        selection.base_event_range(),
                        selection.target_event_range(),
                    )?;
                (state, transferred)
            } else if let Some(selection) = block_selection {
                let transferred = exact_base_transferred_record_count_with_block_splice(
                    runtime.producer_arena(),
                    target_publication,
                    witness.target_page_range(),
                    selection,
                )?;
                let state = CandidateSnapshotEncoderState::new_exact_base_delta_with_block_splice(
                    runtime.producer_arena(),
                    base_publication,
                    target_publication,
                    witness.base_page_range().clone(),
                    witness.target_page_range().clone(),
                    selection.base_entry_range(),
                    selection.target_entry_range(),
                )?;
                (state, transferred)
            } else {
                let transferred = exact_base_transferred_record_count(
                    runtime.producer_arena(),
                    target_publication,
                    witness.target_page_range(),
                )?;
                let state = CandidateSnapshotEncoderState::new_exact_base_delta(
                    runtime.producer_arena(),
                    base_publication,
                    target_publication,
                    witness.base_page_range().clone(),
                    witness.target_page_range().clone(),
                )?;
                (state, transferred)
            };
            Ok((state, transferred_canonical_record_count))
        })();
        let (state, transferred_canonical_record_count) = match setup {
            Ok(setup) => setup,
            Err(error) => {
                return Err(M11ExactBaseSnapshotStreamStartFailure {
                    error,
                    target: self,
                    base,
                    witness: Some(witness),
                })
            }
        };
        if let Err(error) = runtime.take_persistent_source_facts_delta(witness) {
            return Err(M11ExactBaseSnapshotStreamStartFailure {
                error: error.into(),
                target: self,
                base,
                witness: None,
            });
        }

        let mut target_owner = self;
        let mut base_owner = base;
        let runtime_identity = target_owner.runtime_identity;
        let publication = target_owner.publication.take();
        let closing_root = target_owner.closing_root.take();
        let exact_base_publication = base_owner.publication.take();
        let exact_base_closing_root = base_owner.closing_root.take();
        let reference_winner = base_owner.reference_winner.take();
        target_owner.close_complete = true;
        base_owner.close_complete = true;
        Ok(M11OwnedSnapshotStream {
            runtime_identity,
            publication,
            closing_root,
            exact_base_publication,
            exact_base_closing_root,
            reference_winner,
            close_complete: false,
            traversal_complete: false,
            cancelled_for_exact_base_restore: false,
            transferred_canonical_record_count,
            state,
        })
    }

    pub fn begin_close(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        begin_publication_release(
            runtime.producer_arena_mut(),
            &mut self.publication,
            &mut self.closing_root,
        )
    }

    pub fn poll_close(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<bool, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        let complete = poll_publication_release(
            runtime.producer_arena_mut(),
            &self.publication,
            &self.closing_root,
            fuel,
        )?;
        self.close_complete = complete;
        Ok(complete)
    }

    fn publication(&self) -> Result<&PublishedManifest, M11PublicationError> {
        self.publication
            .as_ref()
            .ok_or_else(M11PublicationError::invalid_state)
    }
}

/// Exact semantic start for one bounded retained-publication block visit.
///
/// The three coordinates are one indivisible authentication claim. The
/// retained measured sequence rejects a matching ordinal paired with stale or
/// caller-guessed byte/UTF-16 cuts before yielding any entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RetainedBlockVisitStart {
    entry_ordinal: u64,
    byte_offset: u64,
    utf16_offset: u64,
}

impl M11RetainedBlockVisitStart {
    #[must_use]
    pub const fn new(entry_ordinal: u64, byte_offset: u64, utf16_offset: u64) -> Self {
        Self {
            entry_ordinal,
            byte_offset,
            utf16_offset,
        }
    }

    #[must_use]
    pub const fn entry_ordinal(self) -> u64 {
        self.entry_ordinal
    }

    #[must_use]
    pub const fn byte_offset(self) -> u64 {
        self.byte_offset
    }

    #[must_use]
    pub const fn utf16_offset(self) -> u64 {
        self.utf16_offset
    }
}

/// Whether a retained-publication range visitor should continue after one
/// synchronously borrowed semantic entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RetainedBlockVisitControl {
    Continue,
    Stop,
}

/// One authenticated semantic entry borrowed during a bounded retained visit.
///
/// Only semantic identity, exact source geometry, and typed role data cross
/// this parser-internal seam. Packed-page identity and tree cursors remain
/// private to the engine.
#[derive(Clone, Copy, Debug)]
pub struct M11RetainedBlockVisitEntry<'entry> {
    inner: M11BlockSequenceVisitEntry<'entry>,
}

impl<'entry> M11RetainedBlockVisitEntry<'entry> {
    #[must_use]
    pub const fn entry_ordinal(self) -> u64 {
        self.inner.entry_ordinal()
    }

    #[must_use]
    pub const fn byte_range(self) -> Range<u64> {
        self.inner.byte_start()..self.inner.byte_end()
    }

    #[must_use]
    pub const fn utf16_range(self) -> Range<u64> {
        self.inner.utf16_start()..self.inner.utf16_end()
    }

    #[must_use]
    pub const fn entry(self) -> &'entry M11BlockSequenceEntry {
        self.inner.entry()
    }
}

/// Why one bounded retained-publication block visit returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RetainedBlockVisitDisposition {
    Complete,
    EntryLimit,
    StoragePageLimit,
    VisitorStopped,
}

/// Exact continuation and authenticated work receipt for one retained visit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RetainedBlockVisitReceipt {
    visited_entries: u64,
    storage_pages_visited: u64,
    next_entry_ordinal: u64,
    next_byte_offset: u64,
    next_utf16_offset: u64,
    disposition: M11RetainedBlockVisitDisposition,
    node_headers_decoded: u64,
    summary_combinations: u64,
    payload_bytes_inspected: u64,
    entries_authenticated: u64,
}

impl M11RetainedBlockVisitReceipt {
    #[must_use]
    pub const fn visited_entries(self) -> u64 {
        self.visited_entries
    }

    #[must_use]
    pub const fn storage_pages_visited(self) -> u64 {
        self.storage_pages_visited
    }

    #[must_use]
    pub const fn next_entry_ordinal(self) -> u64 {
        self.next_entry_ordinal
    }

    #[must_use]
    pub const fn next_byte_offset(self) -> u64 {
        self.next_byte_offset
    }

    #[must_use]
    pub const fn next_utf16_offset(self) -> u64 {
        self.next_utf16_offset
    }

    #[must_use]
    pub const fn continuation(self) -> M11RetainedBlockVisitStart {
        M11RetainedBlockVisitStart::new(
            self.next_entry_ordinal,
            self.next_byte_offset,
            self.next_utf16_offset,
        )
    }

    #[must_use]
    pub const fn disposition(self) -> M11RetainedBlockVisitDisposition {
        self.disposition
    }

    #[must_use]
    pub const fn node_headers_decoded(self) -> u64 {
        self.node_headers_decoded
    }

    #[must_use]
    pub const fn summary_combinations(self) -> u64 {
        self.summary_combinations
    }

    #[must_use]
    pub const fn payload_bytes_inspected(self) -> u64 {
        self.payload_bytes_inspected
    }

    #[must_use]
    pub const fn entries_authenticated(self) -> u64 {
        self.entries_authenticated
    }
}

enum M11ReferenceWinnerState {
    Uninitialized,
    Building(ReferenceWinnerIndexBuilder),
    Ready(Arc<ReferenceWinnerIndex>),
    Releasing(ReferenceWinnerIndexReclaimer),
    Released,
}

impl fmt::Debug for M11ReferenceWinnerState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uninitialized => formatter.write_str("Uninitialized"),
            Self::Building(builder) => builder.fmt(formatter),
            Self::Ready(index) => index.fmt(formatter),
            Self::Releasing(reclaimer) => reclaimer.fmt(formatter),
            Self::Released => formatter.write_str("Released"),
        }
    }
}

struct M11ReferenceWinnerHandle {
    runtime_identity: StrongIdentity,
    root: crate::ArenaId,
    state: M11ReferenceWinnerState,
    reservation: Option<ExternalPayloadReservation>,
}

impl fmt::Debug for M11ReferenceWinnerHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ReferenceWinnerHandle")
            .field("runtime_identity", &self.runtime_identity)
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl M11ReferenceWinnerHandle {
    fn new(runtime_identity: StrongIdentity, root: crate::ArenaId) -> Self {
        Self {
            runtime_identity,
            root,
            state: M11ReferenceWinnerState::Uninitialized,
            reservation: None,
        }
    }

    fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        authority: CandidateAuthority,
        fuel: usize,
    ) -> Result<M11ReferenceResolverPoll, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        if fuel == 0 {
            return Err(M11PublicationError::zero_fuel());
        }
        let mut transitions = 0_usize;
        if matches!(self.state, M11ReferenceWinnerState::Uninitialized) {
            let builder =
                ReferenceWinnerIndexBuilder::new(runtime.producer_arena(), authority, self.root)?;
            let reservation = runtime
                .producer_arena_mut()
                .reserve_external_payload(builder.maximum_external_payload_bytes()?)?;
            self.reservation = Some(reservation);
            self.state = M11ReferenceWinnerState::Building(builder);
            transitions = 1;
        }
        if transitions < fuel {
            if let M11ReferenceWinnerState::Building(builder) = &mut self.state {
                let polled = builder.poll(
                    runtime.producer_arena(),
                    authority,
                    self.root,
                    fuel - transitions,
                )?;
                transitions = transitions
                    .checked_add(polled.transitions)
                    .ok_or_else(M11PublicationError::invalid_state)?;
                if polled.complete {
                    let building =
                        std::mem::replace(&mut self.state, M11ReferenceWinnerState::Uninitialized);
                    let M11ReferenceWinnerState::Building(builder) = building else {
                        return Err(M11PublicationError::invalid_state());
                    };
                    self.state = M11ReferenceWinnerState::Ready(Arc::new(builder.into_index()?));
                }
            }
        }
        let (ready, occurrence_count, indexed_occurrences, unique_label_count) = match &self.state {
            M11ReferenceWinnerState::Ready(index) => (
                true,
                index.occurrence_count(),
                index.indexed_occurrences(),
                index.unique_label_count(),
            ),
            M11ReferenceWinnerState::Uninitialized | M11ReferenceWinnerState::Building(_) => {
                (false, 0, 0, 0)
            }
            M11ReferenceWinnerState::Releasing(_) | M11ReferenceWinnerState::Released => {
                return Err(M11PublicationError::invalid_state())
            }
        };
        Ok(M11ReferenceResolverPoll {
            transitions,
            ready,
            occurrence_count,
            indexed_occurrences,
            unique_label_count,
        })
    }

    const fn ready_index(&self) -> Option<&Arc<ReferenceWinnerIndex>> {
        match &self.state {
            M11ReferenceWinnerState::Ready(index) => Some(index),
            M11ReferenceWinnerState::Uninitialized | M11ReferenceWinnerState::Building(_) => None,
            M11ReferenceWinnerState::Releasing(_) | M11ReferenceWinnerState::Released => None,
        }
    }

    fn begin_release(&mut self) -> Result<(), M11PublicationError> {
        let state = std::mem::replace(&mut self.state, M11ReferenceWinnerState::Released);
        self.state = match state {
            M11ReferenceWinnerState::Uninitialized => M11ReferenceWinnerState::Released,
            M11ReferenceWinnerState::Building(builder) => {
                M11ReferenceWinnerState::Releasing(builder.into_reclaimer())
            }
            M11ReferenceWinnerState::Ready(index) => match Arc::try_unwrap(index) {
                Ok(index) => M11ReferenceWinnerState::Releasing(index.into_reclaimer()),
                Err(index) => {
                    self.state = M11ReferenceWinnerState::Ready(index);
                    return Err(M11PublicationError::invalid_state());
                }
            },
            M11ReferenceWinnerState::Releasing(reclaimer) => {
                M11ReferenceWinnerState::Releasing(reclaimer)
            }
            M11ReferenceWinnerState::Released => M11ReferenceWinnerState::Released,
        };
        Ok(())
    }

    fn poll_release(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<(usize, bool), M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        if fuel == 0 {
            return Err(M11PublicationError::zero_fuel());
        }
        let (transitions, complete) = match &mut self.state {
            M11ReferenceWinnerState::Releasing(reclaimer) => {
                let poll = reclaimer.poll(fuel)?;
                (poll.transitions, poll.complete)
            }
            M11ReferenceWinnerState::Released => (0, true),
            M11ReferenceWinnerState::Uninitialized
            | M11ReferenceWinnerState::Building(_)
            | M11ReferenceWinnerState::Ready(_) => return Err(M11PublicationError::invalid_state()),
        };
        if complete {
            self.state = M11ReferenceWinnerState::Released;
            if let Some(reservation) = self.reservation.take() {
                if let Err(failure) = runtime
                    .producer_arena_mut()
                    .release_external_payload(reservation)
                {
                    self.reservation = Some(failure.reservation);
                    return Err(failure.error.into());
                }
            }
        }
        Ok((transitions, complete))
    }
}

impl Drop for M11ReferenceWinnerHandle {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            debug_assert!(
                matches!(self.state, M11ReferenceWinnerState::Released)
                    && self.reservation.is_none(),
                "reference winner handle requires explicit fuelled release"
            );
        }
    }
}

/// Receipt for one bounded reference-winner acceleration quantum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ReferenceResolverPoll {
    transitions: usize,
    ready: bool,
    occurrence_count: u64,
    indexed_occurrences: u64,
    unique_label_count: u64,
}

impl M11ReferenceResolverPoll {
    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }

    #[must_use]
    pub const fn ready(self) -> bool {
        self.ready
    }

    #[must_use]
    pub const fn occurrence_count(self) -> u64 {
        self.occurrence_count
    }

    #[must_use]
    pub const fn indexed_occurrences(self) -> u64 {
        self.indexed_occurrences
    }

    #[must_use]
    pub const fn unique_label_count(self) -> u64 {
        self.unique_label_count
    }
}

/// Cooked target authority for one exact reference-label winner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11ResolvedReference {
    definition_ordinal: u64,
    destination_source: Range<u64>,
    title_source: Option<Range<u64>>,
    cooked_destination: Box<str>,
    cooked_title: Option<Box<str>>,
}

/// Definitive result of one root-bound normalized-label lookup.
///
/// `ValueTooLarge` is intentionally distinct from `Missing`: the former is a
/// real CommonMark reference whose cooked payload cannot fit the caller's
/// bounded sidecar envelope. Inline parsing must fail that leaf closed rather
/// than misclassifying the use as literal text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum M11ReferenceResolution {
    Missing,
    ValueTooLarge,
    Resolved(M11ResolvedReference),
}

impl M11ResolvedReference {
    #[must_use]
    pub const fn definition_ordinal(&self) -> u64 {
        self.definition_ordinal
    }

    #[must_use]
    pub const fn destination_source(&self) -> &Range<u64> {
        &self.destination_source
    }

    #[must_use]
    pub const fn title_source(&self) -> Option<&Range<u64>> {
        self.title_source.as_ref()
    }

    #[must_use]
    pub const fn cooked_destination(&self) -> &str {
        &self.cooked_destination
    }

    #[must_use]
    pub fn cooked_title(&self) -> Option<&str> {
        self.cooked_title.as_deref()
    }
}

/// Cloneable, root-bound lookup capability minted only by a retained exact
/// publication after the winner index has completed. The capability owns no
/// arena pages; its caller must keep the associated publication alive.
#[derive(Clone)]
pub struct M11ReferenceResolver {
    runtime_identity: StrongIdentity,
    authority: CandidateAuthority,
    root: crate::ArenaId,
    index: Arc<ReferenceWinnerIndex>,
}

impl fmt::Debug for M11ReferenceResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ReferenceResolver")
            .field("runtime_identity", &self.runtime_identity)
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl M11ReferenceResolver {
    /// Resolves one already-normalized exact label. Oversized cooked values
    /// return `ValueTooLarge` before allocation so a hostile definition cannot
    /// escape the caller's bounded sidecar envelope or masquerade as missing.
    pub fn resolve(
        &self,
        runtime: &DocumentRuntime,
        normalized_label: &str,
        maximum_cooked_bytes: usize,
    ) -> Result<M11ReferenceResolution, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        if self.index.root() != self.root {
            return Err(M11PublicationError::invalid_state());
        }
        let Some(winner) = self.index.winner(
            runtime.producer_arena(),
            self.authority,
            self.root,
            normalized_label.as_bytes(),
        )?
        else {
            return Ok(M11ReferenceResolution::Missing);
        };
        let destination_len = usize::try_from(winner.cooked_destination.len())
            .map_err(|_| M11PublicationError::invalid_state())?;
        let title_len = winner
            .cooked_title
            .as_ref()
            .map(|title| usize::try_from(title.len()))
            .transpose()
            .map_err(|_| M11PublicationError::invalid_state())?
            .unwrap_or(0);
        let maximum_cooked_bytes = maximum_cooked_bytes.min(
            crate::inline_projection::M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES.saturating_sub(32),
        );
        if destination_len
            .checked_add(title_len)
            .is_none_or(|total| total > maximum_cooked_bytes)
        {
            return Ok(M11ReferenceResolution::ValueTooLarge);
        }
        let cooked_destination = read_reference_utf8(winner.cooked_destination)?;
        let cooked_title = winner.cooked_title.map(read_reference_utf8).transpose()?;
        Ok(M11ReferenceResolution::Resolved(M11ResolvedReference {
            definition_ordinal: winner.ordinal,
            destination_source: winner.destination_source.bytes,
            title_source: winner.title_source.map(|source| source.bytes),
            cooked_destination,
            cooked_title,
        }))
    }
}

fn read_reference_utf8(
    value: crate::reference_root::PersistentBytesView<'_>,
) -> Result<Box<str>, M11PublicationError> {
    let len = usize::try_from(value.len()).map_err(|_| M11PublicationError::invalid_state())?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| M11PublicationError(ErrorInner::Arena(ArenaError::AllocationFailed)))?;
    bytes.resize(len, 0);
    if value.read(0, &mut bytes)? != len {
        return Err(M11PublicationError::invalid_state());
    }
    String::from_utf8(bytes)
        .map(String::into_boxed_str)
        .map_err(|_| M11PublicationError::invalid_state())
}

/// One completely traversed producer publication retained for exact-base use
/// and bounded same-revision semantic refinement.
///
/// This capability can only be recovered from an owned snapshot stream after
/// its terminal frame has been emitted. It exposes no second snapshot
/// traversal: a delivered publication remains sealed. Exact point queries may
/// authenticate one logarithmic path and one bounded packed block page for
/// urgent caret demand, while the parser-internal consecutive visitor performs
/// one bounded semantic range walk for sibling viewport work without exposing
/// storage topology.
pub struct M11RetainedCandidatePublication {
    runtime_identity: StrongIdentity,
    publication: Option<PublishedManifest>,
    closing_root: Option<CommittedArenaRoot>,
    reference_winner: Option<M11ReferenceWinnerHandle>,
    close_complete: bool,
}

impl Drop for M11RetainedCandidatePublication {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            debug_assert!(
                self.close_complete
                    && self.publication.is_none()
                    && self.closing_root.is_none()
                    && self.reference_winner.is_none(),
                "retained M1.1 publication must fuel-close its runtime-owned root"
            );
        }
    }
}

impl M11RetainedCandidatePublication {
    /// Advances the root-bound reference winner acceleration under caller
    /// fuel. Exact-base publications sharing the same canonical References
    /// root move this progress through the exact-stream capability chain.
    pub fn poll_reference_resolver(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ReferenceResolverPoll, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        let publication = self.publication()?;
        let authority = publication.authority();
        let descriptor =
            decode_manifest_descriptor(runtime.producer_arena(), publication.root_id(), authority)?;
        let root =
            persistent_reference_manifest_root(runtime.producer_arena(), &descriptor, authority)?;
        let winner = self
            .reference_winner
            .get_or_insert_with(|| M11ReferenceWinnerHandle::new(self.runtime_identity, root));
        if winner.root != root {
            return Err(M11PublicationError::invalid_state());
        }
        winner.poll(runtime, authority, fuel)
    }

    /// Returns a cheap lookup capability once the fuelled index is ready.
    pub fn reference_resolver(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<Option<M11ReferenceResolver>, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        let publication = self.publication()?;
        let Some(winner) = self.reference_winner.as_ref() else {
            return Ok(None);
        };
        let Some(index) = winner.ready_index() else {
            return Ok(None);
        };
        let authority = publication.authority();
        let descriptor =
            decode_manifest_descriptor(runtime.producer_arena(), publication.root_id(), authority)?;
        let root =
            persistent_reference_manifest_root(runtime.producer_arena(), &descriptor, authority)?;
        if winner.root != root {
            return Err(M11PublicationError::invalid_state());
        }
        Ok(Some(M11ReferenceResolver {
            runtime_identity: self.runtime_identity,
            authority,
            root,
            index: Arc::clone(index),
        }))
    }

    /// Moves reference-index progress from a superseded exact base into the
    /// delivered target after proving that both publications retain the same
    /// canonical References root. Cancellation before delivery keeps the
    /// handle on the base instead, so no shared mutable cache is required.
    #[doc(hidden)]
    pub fn adopt_exact_base_reference_resolver(
        &mut self,
        base: &mut Self,
    ) -> Result<(), M11PublicationError> {
        if self.runtime_identity != base.runtime_identity || self.reference_winner.is_some() {
            return Err(M11PublicationError::invalid_state());
        }
        if base.reference_winner.is_none() {
            return Ok(());
        }
        // The only producer of this base/target pair is exact-stream setup,
        // which authenticated equal canonical References roots before moving
        // the handle into the stream. Terminal detachment moved that same
        // handle into `base`; neither publication can change while sealed.
        self.reference_winner = base.reference_winner.take();
        Ok(())
    }

    pub fn descriptor(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<M11CandidateDescriptor, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        descriptor_from_publication(runtime.producer_arena(), self.publication()?)
    }

    /// Constructs one target block root by path-copying the parser-selected
    /// semantic range from this exact retained publication.
    ///
    /// The retained manifest stays owned by the caller. Only the replacement
    /// block pages and changed measured-tree paths are allocated for the
    /// returned target root.
    pub fn splice_block_sequence_atomic(
        &self,
        runtime: &mut DocumentRuntime,
        target_lease: crate::SourceSnapshotLease,
        selection: &M11BlockSequenceSpliceSelection,
        replacement: &[M11BlockSequenceEntry],
    ) -> Result<(M11BlockSequenceRoot, M11BlockSequenceSpliceReceipt), M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        let publication = self.publication()?;
        let authority = publication.authority();
        let descriptor =
            decode_manifest_descriptor(runtime.producer_arena(), publication.root_id(), authority)?;
        let blocks =
            persistent_block_manifest_roles(runtime.producer_arena(), &descriptor, authority)?;
        splice_persistent_m11_block_sequence_atomic(
            runtime,
            blocks.root,
            blocks.claim,
            target_lease,
            selection,
            replacement,
        )
        .map_err(Into::into)
    }

    /// Locates one exact block at a late same-revision editor point.
    ///
    /// This is a bounded refinement query, not a second publication
    /// traversal: it authenticates one measured-tree path and scans at most
    /// one packed block page. The producer runtime must still own the exact
    /// current source named by this retained publication, including the
    /// supplied byte/UTF-16 scalar boundary.
    pub fn locate_block_point(
        &self,
        runtime: &DocumentRuntime,
        point: M11BlockSequencePoint,
    ) -> Result<Option<M11BlockSequenceLocation>, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        let publication = self.publication()?;
        let authority = publication.authority();
        let current = runtime
            .current_source_version()
            .ok_or_else(M11PublicationError::invalid_state)?;
        if current.root().get() != authority.source_root.get()
            || current.revision().get() != authority.source_revision.get()
            || u64::try_from(current.byte_len()).ok() != Some(authority.source_bytes)
            || u64::try_from(current.utf16_len()).ok() != Some(authority.source_utf16)
        {
            return Err(M11BlockSequenceError::SourceAuthorityMismatch.into());
        }
        let lease = runtime.snapshot_current_source()?;
        let actual_utf16 = lease
            .utf16_offset_for_byte(point.byte_offset())
            .map_err(M11BlockSequenceError::from)?;
        if actual_utf16 != point.utf16_offset() {
            return Err(M11BlockSequenceError::InvalidPoint.into());
        }
        let descriptor =
            decode_manifest_descriptor(runtime.producer_arena(), publication.root_id(), authority)?;
        let blocks =
            persistent_block_manifest_roles(runtime.producer_arena(), &descriptor, authority)?;
        persistent_m11_block_locate_point(
            runtime.producer_arena(),
            blocks.root,
            blocks.claim,
            point,
        )
        .map_err(Into::into)
    }

    /// Visits one consecutive block range from the sealed producer
    /// publication after authenticating the exact current source once.
    ///
    /// This is the producer-side sibling of the imported host range visitor:
    /// it performs one measured-tree seek, then walks consecutive packed
    /// leaves under independent semantic-entry and storage-page caps. The
    /// callback is synchronous, so neither arena cursors nor borrowed records
    /// can escape the retained publication.
    pub fn visit_blocks(
        &self,
        runtime: &DocumentRuntime,
        start: M11RetainedBlockVisitStart,
        maximum_entries: u32,
        maximum_storage_pages: u32,
        mut visitor: impl FnMut(M11RetainedBlockVisitEntry<'_>) -> M11RetainedBlockVisitControl,
    ) -> Result<M11RetainedBlockVisitReceipt, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        let publication = self.publication()?;
        let authority = publication.authority();
        let current = runtime
            .current_source_version()
            .ok_or_else(M11PublicationError::invalid_state)?;
        if current.root().get() != authority.source_root.get()
            || current.revision().get() != authority.source_revision.get()
            || u64::try_from(current.byte_len()).ok() != Some(authority.source_bytes)
            || u64::try_from(current.utf16_len()).ok() != Some(authority.source_utf16)
        {
            return Err(M11BlockSequenceError::SourceAuthorityMismatch.into());
        }
        let descriptor =
            decode_manifest_descriptor(runtime.producer_arena(), publication.root_id(), authority)?;
        let blocks =
            persistent_block_manifest_roles(runtime.producer_arena(), &descriptor, authority)?;
        let receipt = persistent_m11_block_visit_entries(
            runtime.producer_arena(),
            blocks.root,
            blocks.claim,
            M11BlockSequenceVisitStart {
                entry_ordinal: start.entry_ordinal,
                byte_offset: start.byte_offset,
                utf16_offset: start.utf16_offset,
            },
            maximum_entries,
            maximum_storage_pages,
            |entry| match visitor(M11RetainedBlockVisitEntry { inner: entry }) {
                M11RetainedBlockVisitControl::Continue => M11BlockSequenceVisitControl::Continue,
                M11RetainedBlockVisitControl::Stop => M11BlockSequenceVisitControl::Stop,
            },
        )?;
        let inspection = receipt.inspection();
        Ok(M11RetainedBlockVisitReceipt {
            visited_entries: receipt.visited_entries(),
            storage_pages_visited: receipt.storage_pages_visited(),
            next_entry_ordinal: receipt.next_entry_ordinal(),
            next_byte_offset: receipt.next_byte_offset(),
            next_utf16_offset: receipt.next_utf16_offset(),
            disposition: match receipt.disposition() {
                M11BlockSequenceVisitDisposition::Complete => {
                    M11RetainedBlockVisitDisposition::Complete
                }
                M11BlockSequenceVisitDisposition::EntryLimit => {
                    M11RetainedBlockVisitDisposition::EntryLimit
                }
                M11BlockSequenceVisitDisposition::StoragePageLimit => {
                    M11RetainedBlockVisitDisposition::StoragePageLimit
                }
                M11BlockSequenceVisitDisposition::VisitorStopped => {
                    M11RetainedBlockVisitDisposition::VisitorStopped
                }
            },
            node_headers_decoded: inspection.node_headers_decoded,
            summary_combinations: inspection.summary_combinations,
            payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
            entries_authenticated: inspection.spec.spec_items_hashed,
        })
    }

    /// Locates one block in the retained exact base by its authenticated byte
    /// coverage only.
    ///
    /// This does not claim that the base is the runtime's current source. It
    /// exists solely for conservative exact-base splice planning, where the
    /// measured base derives UTF-16 geometry and edit lineage independently
    /// authenticates the eventual prefix and suffix reuse cuts.
    #[doc(hidden)]
    pub fn locate_exact_base_block_byte(
        &self,
        runtime: &DocumentRuntime,
        byte_offset: usize,
        affinity: crate::SourceBoundaryAffinity,
    ) -> Result<Option<M11BlockSequenceLocation>, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        let publication = self.publication()?;
        let authority = publication.authority();
        let descriptor =
            decode_manifest_descriptor(runtime.producer_arena(), publication.root_id(), authority)?;
        let blocks =
            persistent_block_manifest_roles(runtime.producer_arena(), &descriptor, authority)?;
        persistent_m11_block_locate_byte(
            runtime.producer_arena(),
            blocks.root,
            blocks.claim,
            byte_offset,
            affinity,
        )
        .map_err(Into::into)
    }

    /// Mints one exact-base hot-inline sidecar binding from the retained
    /// canonical publication capability.
    ///
    /// This is intentionally the only public constructor: parser callers
    /// cannot manufacture candidate authority or attach an inline root to a
    /// merely similar source revision.
    // Keeping the four byte/UTF-16 ranges explicit mirrors the authenticated
    // binding contract; grouping them here would add a one-call wrapper type.
    #[allow(clippy::too_many_arguments)]
    pub fn hot_inline_sidecar_binding(
        &self,
        runtime: &DocumentRuntime,
        parser_profile: ParserProfileId,
        refinement_generation: u64,
        block_ordinal: u64,
        physical_range: Range<u32>,
        visible_range: Range<u32>,
        physical_range_utf16: Range<u32>,
        visible_range_utf16: Range<u32>,
    ) -> Result<M11HotInlineSidecarBinding, M11PublicationError> {
        self.hot_inline_sidecar_binding_for_owner(
            runtime,
            parser_profile,
            refinement_generation,
            M11InlineOverlayOwner::BlockOrdinal(block_ordinal),
            physical_range,
            visible_range,
            physical_range_utf16,
            visible_range_utf16,
        )
    }

    /// Mints one exact-base sidecar binding for a Paragraph owner selected by
    /// the retained recursive-Green publication rather than the flat block
    /// sequence.
    #[allow(clippy::too_many_arguments)]
    pub fn recursive_green_hot_inline_sidecar_binding(
        &self,
        runtime: &DocumentRuntime,
        parser_profile: ParserProfileId,
        refinement_generation: u64,
        frame: M11RecursiveGreenFrameId,
        physical_range: Range<u32>,
        visible_range: Range<u32>,
        physical_range_utf16: Range<u32>,
        visible_range_utf16: Range<u32>,
    ) -> Result<M11HotInlineSidecarBinding, M11PublicationError> {
        self.hot_inline_sidecar_binding_for_owner(
            runtime,
            parser_profile,
            refinement_generation,
            M11InlineOverlayOwner::RecursiveGreenFrame(frame.get()),
            physical_range,
            visible_range,
            physical_range_utf16,
            visible_range_utf16,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn hot_inline_sidecar_binding_for_owner(
        &self,
        runtime: &DocumentRuntime,
        parser_profile: ParserProfileId,
        refinement_generation: u64,
        owner: M11InlineOverlayOwner,
        physical_range: Range<u32>,
        visible_range: Range<u32>,
        physical_range_utf16: Range<u32>,
        visible_range_utf16: Range<u32>,
    ) -> Result<M11HotInlineSidecarBinding, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        let publication = self.publication()?;
        let source = runtime
            .current_source_version()
            .ok_or_else(M11PublicationError::invalid_state)?;
        let base = M11InlineOverlayBase::new(publication.authority(), source, parser_profile)
            .map_err(M11InlineOverlayTransportError::from)?;
        let binding = M11InlineOverlayBinding::new(
            base,
            refinement_generation,
            owner,
            physical_range,
            visible_range,
            physical_range_utf16,
            visible_range_utf16,
        )
        .map_err(M11InlineOverlayTransportError::from)?;
        Ok(M11HotInlineSidecarBinding(binding))
    }

    pub fn begin_close(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        if let Some(winner) = self.reference_winner.as_mut() {
            winner.begin_release()?;
        }
        begin_publication_release(
            runtime.producer_arena_mut(),
            &mut self.publication,
            &mut self.closing_root,
        )
    }

    pub fn poll_close(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<bool, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        if fuel == 0 {
            return Err(M11PublicationError::zero_fuel());
        }
        let mut remaining = fuel;
        if let Some(winner) = self.reference_winner.as_mut() {
            let (consumed, complete) = winner.poll_release(runtime, remaining)?;
            remaining = remaining
                .checked_sub(consumed)
                .ok_or_else(M11PublicationError::invalid_state)?;
            if !complete {
                return Ok(false);
            }
            drop(self.reference_winner.take());
            if remaining == 0 {
                return Ok(false);
            }
        }
        let complete = poll_publication_release(
            runtime.producer_arena_mut(),
            &self.publication,
            &self.closing_root,
            remaining,
        )?;
        self.close_complete = complete;
        Ok(complete)
    }

    fn publication(&self) -> Result<&PublishedManifest, M11PublicationError> {
        self.publication
            .as_ref()
            .ok_or_else(M11PublicationError::invalid_state)
    }
}

/// Recoverable failure to allocate or validate owned snapshot traversal state.
/// The sealed producer remains boxed so callers can fuel-close it explicitly.
pub struct M11SnapshotStreamStartFailure {
    error: M11PublicationError,
    publication: Box<M11CandidatePublication>,
}

impl M11SnapshotStreamStartFailure {
    pub fn into_parts(self) -> (M11PublicationError, Box<M11CandidatePublication>) {
        (self.error, self.publication)
    }
}

impl fmt::Debug for M11SnapshotStreamStartFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11SnapshotStreamStartFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

/// Recoverable ownership receipt for an exact-base stream start failure.
///
/// A present witness means validation failed before the runtime consumed its
/// one-use eligibility. `None` means the runtime rejected and consumed the
/// witness; callers must fail closed rather than silently retrying a full
/// snapshot for the same candidate.
pub struct M11ExactBaseSnapshotStreamStartFailure {
    error: M11PublicationError,
    target: Box<M11CandidatePublication>,
    base: Box<M11RetainedCandidatePublication>,
    witness: Option<Box<PersistentSourceFactsDeltaWitness>>,
}

impl M11ExactBaseSnapshotStreamStartFailure {
    pub fn into_parts(
        self,
    ) -> (
        M11PublicationError,
        Box<M11CandidatePublication>,
        Box<M11RetainedCandidatePublication>,
        Option<Box<PersistentSourceFactsDeltaWitness>>,
    ) {
        (self.error, self.target, self.base, self.witness)
    }
}

impl fmt::Debug for M11ExactBaseSnapshotStreamStartFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ExactBaseSnapshotStreamStartFailure")
            .field("error", &self.error)
            .field("witness_consumed", &self.witness.is_none())
            .finish_non_exhaustive()
    }
}

fn validate_exact_base_witness(
    arena: &PageArena,
    base: &PublishedManifest,
    target: &PublishedManifest,
    witness: &PersistentSourceFactsDeltaWitness,
) -> Result<(), M11PublicationError> {
    let parser_profile = u32::try_from(witness.parser_profile().get()).map_err(|_| {
        M11PublicationError(ErrorInner::Invalid(
            "exact-base parser profile exceeds the candidate schema",
        ))
    })?;
    let matches = |authority: CandidateAuthority, source: SourceVersion| {
        authority.source_root == source.root()
            && authority.source_revision == source.revision()
            && authority.source_bytes == source.byte_len() as u64
            && authority.source_utf16 == source.utf16_len() as u64
            && authority.syntax_profile == parser_profile
    };
    if !matches(base.authority(), witness.base()) || !matches(target.authority(), witness.target())
    {
        return Err(M11PublicationError(ErrorInner::Invalid(
            "exact-base witness does not match the retained base and target publications",
        )));
    }

    // Authenticate both complete manifests before the witness can be consumed.
    let _ = decode_manifest_descriptor(arena, base.root_id(), base.authority())?;
    let _ = decode_manifest_descriptor(arena, target.root_id(), target.authority())?;
    Ok(())
}

fn exact_base_transferred_record_count(
    arena: &PageArena,
    target: &PublishedManifest,
    target_page_range: &Range<u64>,
) -> Result<u64, M11PublicationError> {
    let descriptor = decode_manifest_descriptor(arena, target.root_id(), target.authority())?;
    let source_facts = descriptor.metadata[role_index(CandidateRole::SourceFacts)];
    if target_page_range.start > target_page_range.end
        || target_page_range.end > source_facts.record_count
    {
        return Err(M11PublicationError(ErrorInner::Invalid(
            "exact-base target SourceFacts page range is invalid",
        )));
    }
    [
        CandidateRole::Green,
        CandidateRole::Projection,
        CandidateRole::CleanEofOnly,
    ]
    .into_iter()
    .try_fold(
        target_page_range.end - target_page_range.start,
        |total, role| {
            total
                .checked_add(descriptor.metadata[role_index(role)].record_count)
                .ok_or(M11PublicationError(ErrorInner::Invalid(
                    "exact-base transferred record count overflow",
                )))
        },
    )
}

fn exact_base_transferred_record_count_with_block_splice(
    arena: &PageArena,
    target: &PublishedManifest,
    target_page_range: &Range<u64>,
    selection: &M11BlockSequenceSpliceSelection,
) -> Result<u64, M11PublicationError> {
    let descriptor = decode_manifest_descriptor(arena, target.root_id(), target.authority())?;
    let source_facts = descriptor.metadata[role_index(CandidateRole::SourceFacts)];
    if target_page_range.start > target_page_range.end
        || target_page_range.end > source_facts.record_count
    {
        return Err(M11PublicationError(ErrorInner::Invalid(
            "exact-base target SourceFacts page range is invalid",
        )));
    }
    let blocks = persistent_block_manifest_roles(arena, &descriptor, target.authority())?;
    let target_plan = plan_persistent_m11_block_semantic_splice(
        arena,
        blocks.root,
        blocks.claim,
        selection.target_entry_range(),
    )?;
    (target_page_range.end - target_page_range.start)
        .checked_add(
            target_plan
                .storage_page_range
                .end
                .checked_sub(target_plan.storage_page_range.start)
                .ok_or(M11PublicationError(ErrorInner::Invalid(
                    "exact-base target block page range underflow",
                )))?,
        )
        .and_then(|total| {
            total.checked_add(
                descriptor.metadata[role_index(CandidateRole::CleanEofOnly)].record_count,
            )
        })
        .ok_or(M11PublicationError(ErrorInner::Invalid(
            "exact-base transferred record count overflow",
        )))
}

fn exact_base_transferred_record_count_with_recursive_green_splice(
    arena: &PageArena,
    target: &PublishedManifest,
    target_page_range: &Range<u64>,
    selection: M11RecursiveGreenStructuralSpliceSelection,
) -> Result<u64, M11PublicationError> {
    let descriptor = decode_manifest_descriptor(arena, target.root_id(), target.authority())?;
    let source_facts = descriptor.metadata[role_index(CandidateRole::SourceFacts)];
    if target_page_range.start > target_page_range.end
        || target_page_range.end > source_facts.record_count
    {
        return Err(M11PublicationError(ErrorInner::Invalid(
            "exact-base target SourceFacts page range is invalid",
        )));
    }
    let green = persistent_recursive_green_manifest_role(arena, &descriptor, target.authority())?;
    let target_plan = plan_persistent_m11_recursive_green_semantic_splice(
        arena,
        green.root,
        green.descriptor,
        selection.target_event_range(),
    )?;
    (target_page_range.end - target_page_range.start)
        .checked_add(
            target_plan
                .storage_page_range
                .end
                .checked_sub(target_plan.storage_page_range.start)
                .ok_or(M11PublicationError(ErrorInner::Invalid(
                    "exact-base target recursive Green page range underflow",
                )))?,
        )
        .and_then(|total| {
            total.checked_add(
                descriptor.metadata[role_index(CandidateRole::Projection)].record_count,
            )
        })
        .and_then(|total| {
            total.checked_add(
                descriptor.metadata[role_index(CandidateRole::CleanEofOnly)].record_count,
            )
        })
        .ok_or(M11PublicationError(ErrorInner::Invalid(
            "exact-base transferred record count overflow",
        )))
}

fn descriptor_from_publication(
    arena: &PageArena,
    publication: &PublishedManifest,
) -> Result<M11CandidateDescriptor, M11PublicationError> {
    let authority = publication.authority();
    let descriptor =
        decode_manifest_descriptor(arena, publication.root_id(), publication.authority())?;
    let canonical_record_count = descriptor
        .metadata
        .iter()
        .try_fold(0_u64, |total, role| total.checked_add(role.record_count))
        .ok_or(M11PublicationError(ErrorInner::Invalid(
            "candidate canonical record count overflow",
        )))?;
    let maximum_snapshot_frames = u64::try_from(arena.limits().max_slots)
        .map_err(|_| M11PublicationError(ErrorInner::Invalid("snapshot slot bound overflow")))?
        .checked_add(2)
        .ok_or(M11PublicationError(ErrorInner::Invalid(
            "snapshot frame bound overflow",
        )))?;
    let maximum_snapshot_encoded_bytes = maximum_snapshot_encoded_bytes(arena)?;
    Ok(M11CandidateDescriptor {
        document: authority.document.0,
        publication: authority.publication.0,
        source_root: authority.source_root.get(),
        source_revision: authority.source_revision.get(),
        source_bytes: authority.source_bytes,
        source_utf16: authority.source_utf16,
        parse_generation: authority.parse_generation.get(),
        syntax_profile: authority.syntax_profile,
        canonical_record_count,
        manifest_digest256: manifest_digest256(authority, &descriptor),
        maximum_snapshot_frames,
        maximum_snapshot_encoded_bytes,
    })
}

fn validate_runtime(
    expected: StrongIdentity,
    runtime: &DocumentRuntime,
) -> Result<(), M11PublicationError> {
    if runtime.producer_identity() != expected {
        return Err(M11PublicationError(ErrorInner::Invalid(
            "candidate capability belongs to a different document runtime",
        )));
    }
    Ok(())
}

fn begin_publication_release(
    arena: &mut PageArena,
    publication: &mut Option<PublishedManifest>,
    closing_root: &mut Option<CommittedArenaRoot>,
) -> Result<(), M11PublicationError> {
    let root = if let Some(root) = closing_root.take() {
        root
    } else if let Some(publication) = publication.take() {
        publication.into_root()
    } else {
        return Ok(());
    };
    match arena.release_committed_root(root) {
        Ok(()) => Ok(()),
        Err(failure) => {
            *closing_root = Some(failure.root);
            Err(failure.error.into())
        }
    }
}

fn poll_publication_release(
    arena: &mut PageArena,
    publication: &Option<PublishedManifest>,
    closing_root: &Option<CommittedArenaRoot>,
    fuel: usize,
) -> Result<bool, M11PublicationError> {
    if publication.is_some() || closing_root.is_some() {
        return Err(M11PublicationError::invalid_state());
    }
    if fuel == 0 {
        return Err(M11PublicationError::zero_fuel());
    }
    Ok(arena.poll_reclaim(fuel).complete)
}

/// Conservative encoded-byte ceiling for every snapshot the arena can hold.
///
/// Multiplying the slot count by the maximum *individual* frame size assumes
/// every node simultaneously owns a full payload and the maximum child list.
/// The arena already enforces those resources independently, so that product
/// becomes needlessly larger than the host's wire ceiling at useful slot
/// counts. This bound instead charges every possible node header, every
/// possible child ordinal, the complete live-payload budget, and two complete
/// terminal-frame envelopes. It remains independent of the candidate's live
/// occupancy while preserving the arena's hard limits.
fn maximum_snapshot_encoded_bytes(arena: &PageArena) -> Result<u64, M11PublicationError> {
    let limits = arena.limits();
    let slots = u64::try_from(limits.max_slots)
        .map_err(|_| M11PublicationError(ErrorInner::Invalid("snapshot slot bound overflow")))?;
    let children_per_node = limits.max_children_per_node;
    let header_bytes = slots
        .checked_mul(u64::try_from(SNAPSHOT_NODE_HEADER_BYTES).map_err(|_| {
            M11PublicationError(ErrorInner::Invalid("snapshot header bound overflow"))
        })?)
        .ok_or(M11PublicationError(ErrorInner::Invalid(
            "snapshot header bound overflow",
        )))?;
    let child_bytes = slots
        .checked_mul(u64::try_from(children_per_node).map_err(|_| {
            M11PublicationError(ErrorInner::Invalid("snapshot child bound overflow"))
        })?)
        .and_then(|edges| edges.checked_mul(u64::try_from(SNAPSHOT_CHILD_ORDINAL_BYTES).ok()?))
        .ok_or(M11PublicationError(ErrorInner::Invalid(
            "snapshot child bound overflow",
        )))?;
    let payload_bytes = u64::try_from(limits.max_live_payload_bytes)
        .map_err(|_| M11PublicationError(ErrorInner::Invalid("snapshot payload bound overflow")))?;
    let terminal_bytes = u64::try_from(M11_MAX_SNAPSHOT_FRAME_BYTES)
        .ok()
        .and_then(|bytes| bytes.checked_mul(2))
        .ok_or(M11PublicationError(ErrorInner::Invalid(
            "snapshot terminal bound overflow",
        )))?;
    header_bytes
        .checked_add(child_bytes)
        .and_then(|bytes| bytes.checked_add(payload_bytes))
        .and_then(|bytes| bytes.checked_add(terminal_bytes))
        .ok_or(M11PublicationError(ErrorInner::Invalid(
            "snapshot stream bound overflow",
        )))
}

#[cfg(test)]
mod snapshot_envelope_tests {
    use super::*;
    use crate::document::DocumentRuntimeConfig;
    use crate::identity::SourceRootId;
    use crate::storage::ArenaLimits;
    use crate::SourceRevision;

    fn production_runtime() -> DocumentRuntime {
        DocumentRuntime::new(
            "",
            DocumentRuntimeConfig {
                arena_limits: ArenaLimits {
                    max_slots: M11_CANDIDATE_ARENA_MAX_SLOTS,
                    max_live_payload_bytes: M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES,
                    max_children_per_node: M11_MAX_ROLE_RECORDS,
                },
                ..DocumentRuntimeConfig::default()
            },
        )
        .expect("document runtime")
    }

    fn published_stream(
        runtime: &mut DocumentRuntime,
        publication: [u8; 16],
    ) -> M11OwnedSnapshotStream {
        let records = M11RoleRecords::new(
            [Box::<[u8]>::from(&b"s"[..])],
            Box::<[u8]>::from(&b"g"[..]),
            Box::<[u8]>::from(&b"p"[..]),
        )
        .expect("bounded role records");
        let source = SourceVersion::from_authenticated_parts(
            SourceRevision::new(1),
            SourceRootId::from_wire(1).expect("nonzero source root"),
            1,
            1,
        );
        let mut build =
            M11CandidateBuild::new(runtime, [1; 16], publication, source, 1, 1, records)
                .expect("candidate build");
        build.finish_references(runtime).expect("finish references");
        while let M11CandidateBuildPoll::Pending { .. } =
            build.poll(runtime, 256).expect("candidate build poll")
        {}
        Box::new(build.into_publication().expect("published candidate"))
            .into_snapshot_stream(runtime)
            .expect("owned snapshot stream")
    }

    fn close_runtime(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin runtime close");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    #[test]
    fn candidate_build_uses_the_production_m11_arena_envelope() {
        let records = M11RoleRecords::new(
            [Box::<[u8]>::from(&b"s"[..])],
            Box::<[u8]>::from(&b"g"[..]),
            Box::<[u8]>::from(&b"p"[..]),
        )
        .expect("bounded role records");
        let source = SourceVersion::from_authenticated_parts(
            SourceRevision::new(1),
            SourceRootId::from_wire(1).expect("nonzero source root"),
            1,
            1,
        );
        let expected_limits = ArenaLimits {
            max_slots: M11_CANDIDATE_ARENA_MAX_SLOTS,
            max_live_payload_bytes: M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES,
            max_children_per_node: M11_MAX_ROLE_RECORDS,
        };
        let mut runtime = DocumentRuntime::new(
            "",
            DocumentRuntimeConfig {
                arena_limits: expected_limits,
                ..DocumentRuntimeConfig::default()
            },
        )
        .expect("document runtime");
        let mut build =
            M11CandidateBuild::new(&mut runtime, [1; 16], [2; 16], source, 1, 1, records)
                .expect("candidate build");

        assert_eq!(runtime.producer_arena().limits(), expected_limits);
        build
            .begin_abort(&mut runtime)
            .expect("begin bounded build abort");
        while !build
            .poll_abort(&mut runtime, 256)
            .expect("poll bounded build abort")
        {}
        runtime.begin_close().expect("begin runtime close");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
    }

    #[test]
    fn completed_stream_hands_back_one_move_only_publication_and_fails_closed() {
        let mut runtime = production_runtime();
        let wrong_runtime = production_runtime();
        let mut stream = published_stream(&mut runtime, [3; 16]);

        stream = match stream.into_retained_publication(&runtime) {
            Ok(_) => panic!("incomplete traversal must not become retained"),
            Err(failure) => {
                let (error, stream) = failure.into_parts();
                assert_eq!(
                    error.to_string(),
                    "publication owner is not in the required state"
                );
                stream
            }
        };

        assert_eq!(
            stream.begin_frame().expect("snapshot begin").kind,
            M11SnapshotFrameKind::Begin
        );
        loop {
            match stream.poll(&runtime, 256).expect("snapshot traversal") {
                M11OwnedSnapshotPoll::Pending { .. } => {}
                M11OwnedSnapshotPoll::Frame { frame, .. }
                    if frame.kind == M11SnapshotFrameKind::End =>
                {
                    break;
                }
                M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                    panic!("full snapshot must not require exact-base replay")
                }
                M11OwnedSnapshotPoll::Frame { .. } => {}
            }
        }

        let stream = match stream.into_retained_publication(&wrong_runtime) {
            Ok(_) => panic!("wrong runtime must not accept retained handback"),
            Err(failure) => {
                let (error, stream) = failure.into_parts();
                assert_eq!(
                    error.to_string(),
                    "candidate capability belongs to a different document runtime"
                );
                stream
            }
        };
        let mut retained = stream
            .into_retained_publication(&runtime)
            .expect("completed traversal returns its publication");
        assert_eq!(
            retained
                .descriptor(&runtime)
                .expect("retained descriptor")
                .publication,
            [3; 16]
        );
        retained
            .begin_close(&mut runtime)
            .expect("begin retained close");
        while !retained
            .poll_close(&mut runtime, 1)
            .expect("fuelled retained close")
        {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);

        close_runtime(runtime);
        close_runtime(wrong_runtime);
    }

    #[test]
    fn independent_arena_limits_admit_the_reference_scale_slot_envelope() {
        let limits = ArenaLimits {
            max_slots: M11_CANDIDATE_ARENA_MAX_SLOTS,
            max_live_payload_bytes: 64 * 1024 * 1024,
            max_children_per_node: M11_MAXIMUM_SNAPSHOT_CHILDREN,
        };
        let arena = PageArena::new(limits).expect("bounded arena");
        let encoded = maximum_snapshot_encoded_bytes(&arena).expect("encoded bound");
        let former_frame_product = u64::try_from(limits.max_slots + 2).expect("slot bound")
            * u64::try_from(M11_MAX_SNAPSHOT_FRAME_BYTES).expect("frame bound");

        assert!(encoded < 512 * 1024 * 1024);
        assert!(former_frame_product > 512 * 1024 * 1024);
        assert_eq!(encoded, 203_958_312);
    }
}

/// Stable semantic kind of one producer-owned snapshot frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11SnapshotFrameKind {
    Begin,
    SourceFactsReplacementPage,
    BlockSequenceReplacementPage,
    RecursiveGreenReplacementPage,
    Node,
    End,
}

/// One complete self-contained snapshot frame plus engine-decoded metadata.
///
/// Transport adapters consume the metadata; they never infer canonical record
/// boundaries from private frame tags.
pub struct M11SnapshotFrame {
    pub kind: M11SnapshotFrameKind,
    pub node_ordinal: Option<u64>,
    pub canonical_record_count: u32,
    pub canonical_stream_digest256: Option<[u8; 32]>,
    pub bytes: Box<[u8]>,
}

fn classified_snapshot_frame(bytes: Box<[u8]>) -> Result<M11SnapshotFrame, M11PublicationError> {
    let metadata = classify_snapshot_frame(&bytes)?;
    Ok(M11SnapshotFrame {
        kind: match metadata.kind {
            SnapshotFrameKind::Begin => M11SnapshotFrameKind::Begin,
            SnapshotFrameKind::SourceFactsReplacementPage => {
                M11SnapshotFrameKind::SourceFactsReplacementPage
            }
            SnapshotFrameKind::BlockSequenceReplacementPage => {
                M11SnapshotFrameKind::BlockSequenceReplacementPage
            }
            SnapshotFrameKind::RecursiveGreenReplacementPage => {
                M11SnapshotFrameKind::RecursiveGreenReplacementPage
            }
            SnapshotFrameKind::Node => M11SnapshotFrameKind::Node,
            SnapshotFrameKind::End => M11SnapshotFrameKind::End,
        },
        node_ordinal: metadata.node_ordinal,
        canonical_record_count: metadata.canonical_record_count,
        canonical_stream_digest256: metadata.canonical_stream_digest256,
        bytes,
    })
}

/// Exact candidate and block-fence capability for one late inline sidecar.
///
/// Values are minted only by [`M11RetainedCandidatePublication`].
#[derive(Clone)]
pub struct M11HotInlineSidecarBinding(M11InlineOverlayBinding);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HotInlineSidecarOwner {
    BlockOrdinal(u64),
    RecursiveGreenFrame(M11RecursiveGreenFrameId),
}

impl M11HotInlineSidecarBinding {
    #[must_use]
    pub const fn parser_profile(&self) -> ParserProfileId {
        self.0.base().parser_profile()
    }

    #[must_use]
    pub const fn refinement_generation(&self) -> u64 {
        self.0.generation()
    }

    #[must_use]
    pub const fn owner(&self) -> M11HotInlineSidecarOwner {
        match self.0.owner() {
            M11InlineOverlayOwner::BlockOrdinal(ordinal) => {
                M11HotInlineSidecarOwner::BlockOrdinal(ordinal)
            }
            M11InlineOverlayOwner::RecursiveGreenFrame(frame) => {
                let Some(frame) = M11RecursiveGreenFrameId::new(frame) else {
                    unreachable!()
                };
                M11HotInlineSidecarOwner::RecursiveGreenFrame(frame)
            }
        }
    }

    #[must_use]
    pub const fn block_ordinal(&self) -> Option<u64> {
        match self.owner() {
            M11HotInlineSidecarOwner::BlockOrdinal(ordinal) => Some(ordinal),
            M11HotInlineSidecarOwner::RecursiveGreenFrame(_) => None,
        }
    }

    #[must_use]
    pub const fn physical_range(&self) -> &Range<u32> {
        self.0.physical_range()
    }

    #[must_use]
    pub const fn visible_range(&self) -> &Range<u32> {
        self.0.visible_range()
    }

    #[must_use]
    pub const fn physical_range_utf16(&self) -> &Range<u32> {
        self.0.physical_range_utf16()
    }

    #[must_use]
    pub const fn visible_range_utf16(&self) -> &Range<u32> {
        self.0.visible_range_utf16()
    }
}

/// Semantic summary committed by the engine-owned HIO1 envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HotInlineSidecarDisposition {
    Authoritative {
        logical_page_count: u64,
        fact_count: u64,
        storage_page_count: u64,
        link_value_entry_count: u32,
        link_value_storage_page_count: u64,
        link_value_encoded_bytes: u32,
        ordered_commitment256: [u8; 32],
    },
    IndentedCodeAuthoritative {
        logical_page_count: u64,
        line_count: u64,
        storage_page_count: u64,
        ordered_commitment256: [u8; 32],
    },
    BlockQuoteAuthoritative {
        logical_page_count: u64,
        line_count: u64,
        storage_page_count: u64,
        ordered_commitment256: [u8; 32],
    },
    BulletListAuthoritative {
        /// Present when the sidecar contains only one selected list item.
        selected_item_ordinal: Option<u32>,
        selected_item_line_ending: Option<M11HotInlineCanonicalLineEnding>,
        logical_page_count: u64,
        item_count: u64,
        storage_page_count: u64,
        ordered_commitment256: [u8; 32],
    },
    OrderedListAuthoritative {
        selected_item_ordinal: u32,
        selected_item_line_ending: M11HotInlineCanonicalLineEnding,
        opening_marker_start: u32,
        opening_marker_end: u32,
        marker_value: u32,
        logical_page_count: u64,
        item_count: u64,
        storage_page_count: u64,
        ordered_commitment256: [u8; 32],
    },
    Unsupported {
        reason: u32,
        metadata_commitment256: [u8; 32],
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HotInlineCanonicalLineEnding {
    Lf,
    CrLf,
    Cr,
}

impl From<M11HotInlineCanonicalLineEnding> for M11InlineOverlayCanonicalLineEnding {
    fn from(value: M11HotInlineCanonicalLineEnding) -> Self {
        match value {
            M11HotInlineCanonicalLineEnding::Lf => Self::Lf,
            M11HotInlineCanonicalLineEnding::CrLf => Self::CrLf,
            M11HotInlineCanonicalLineEnding::Cr => Self::Cr,
        }
    }
}

impl From<M11InlineOverlayCanonicalLineEnding> for M11HotInlineCanonicalLineEnding {
    fn from(value: M11InlineOverlayCanonicalLineEnding) -> Self {
        match value {
            M11InlineOverlayCanonicalLineEnding::Lf => Self::Lf,
            M11InlineOverlayCanonicalLineEnding::CrLf => Self::CrLf,
            M11InlineOverlayCanonicalLineEnding::Cr => Self::Cr,
        }
    }
}

/// Bounded transport facts exposed without leaking the private HIO1/IPR3
/// encoders into the bridge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HotInlineSidecarDescriptor {
    hio1_encoded_bytes: u32,
    ipr2_descriptor_bytes: u32,
    transferred_node_count: u32,
    hio1_envelope_digest256: [u8; 32],
    disposition: M11HotInlineSidecarDisposition,
}

impl M11HotInlineSidecarDescriptor {
    #[must_use]
    pub const fn hio1_encoded_bytes(&self) -> u32 {
        self.hio1_encoded_bytes
    }

    #[must_use]
    pub const fn ipr2_descriptor_bytes(&self) -> u32 {
        self.ipr2_descriptor_bytes
    }

    #[must_use]
    pub const fn transferred_node_count(&self) -> u32 {
        self.transferred_node_count
    }

    #[must_use]
    pub const fn hio1_envelope_digest256(&self) -> [u8; 32] {
        self.hio1_envelope_digest256
    }

    #[must_use]
    pub const fn disposition(&self) -> M11HotInlineSidecarDisposition {
        self.disposition
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HotInlineSidecarFrameKind {
    Begin,
    Node,
    End,
}

/// One closed engine-owned sidecar frame.
pub struct M11HotInlineSidecarFrame {
    pub kind: M11HotInlineSidecarFrameKind,
    pub node_ordinal: Option<u64>,
    pub root_stream_digest256: Option<[u8; 32]>,
    pub bytes: Box<[u8]>,
}

pub enum M11HotInlineSidecarSnapshotPoll {
    Pending {
        transitions: usize,
    },
    Frame {
        transitions: usize,
        frame: M11HotInlineSidecarFrame,
    },
}

/// Narrow parser-side adapter over the private HIO1 + arena-closure encoder.
///
/// The producer must retain the corresponding authoritative inline root until
/// this encoder emits End. `poll` revalidates the runtime and exact source on
/// every quantum, so source mutation fails closed.
pub struct M11HotInlineSidecarSnapshotEncoder {
    inner: M11InlineOverlaySnapshotEncoder,
    descriptor: M11HotInlineSidecarDescriptor,
}

impl M11HotInlineSidecarSnapshotEncoder {
    pub fn authoritative(
        runtime: &DocumentRuntime,
        binding: M11HotInlineSidecarBinding,
        projection: &M11InlineProjectionRoot,
    ) -> Result<Self, M11PublicationError> {
        let envelope = M11InlineOverlayEnvelope::from_projection(binding.0.clone(), projection)
            .map_err(M11InlineOverlayTransportError::from)?;
        let transferred_node_count = projection
            .descriptor()
            .storage_page_count()
            .checked_add(projection.descriptor().link_value_storage_page_count())
            .and_then(|nodes| nodes.checked_add(1))
            .and_then(|nodes| u32::try_from(nodes).ok())
            .ok_or(M11PublicationError(ErrorInner::Invalid(
                "hot-inline node count exceeds transport width",
            )))?;
        let descriptor = sidecar_descriptor(
            &envelope,
            transferred_node_count,
            u32::try_from(PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES).expect("IPR3 fits"),
        );
        let inner = M11InlineOverlaySnapshotEncoder::authoritative(runtime, binding.0, projection)?;
        Ok(Self { inner, descriptor })
    }

    pub fn authoritative_indented_code(
        runtime: &DocumentRuntime,
        binding: M11HotInlineSidecarBinding,
        projection: &M11IndentedCodeProjectionRoot,
    ) -> Result<Self, M11PublicationError> {
        let envelope =
            M11InlineOverlayEnvelope::from_indented_code_projection(binding.0.clone(), projection)
                .map_err(M11InlineOverlayTransportError::from)?;
        let descriptor = sidecar_descriptor(
            &envelope,
            u32::try_from(projection.descriptor().storage_page_count()).map_err(|_| {
                M11PublicationError(ErrorInner::Invalid(
                    "indented-code sidecar node count exceeds transport width",
                ))
            })?,
            u32::try_from(PERSISTENT_INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES)
                .expect("indented-code descriptor fits"),
        );
        let inner = M11InlineOverlaySnapshotEncoder::authoritative_indented_code(
            runtime, binding.0, projection,
        )?;
        Ok(Self { inner, descriptor })
    }

    pub fn authoritative_block_quote(
        runtime: &DocumentRuntime,
        binding: M11HotInlineSidecarBinding,
        projection: &M11BlockQuoteProjectionRoot,
    ) -> Result<Self, M11PublicationError> {
        let envelope =
            M11InlineOverlayEnvelope::from_block_quote_projection(binding.0.clone(), projection)
                .map_err(M11InlineOverlayTransportError::from)?;
        let descriptor = sidecar_descriptor(
            &envelope,
            u32::try_from(projection.descriptor().storage_page_count()).map_err(|_| {
                M11PublicationError(ErrorInner::Invalid(
                    "block-quote sidecar node count exceeds transport width",
                ))
            })?,
            u32::try_from(PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES)
                .expect("block-quote descriptor fits"),
        );
        let inner = M11InlineOverlaySnapshotEncoder::authoritative_block_quote(
            runtime, binding.0, projection,
        )?;
        Ok(Self { inner, descriptor })
    }

    pub fn authoritative_bullet_list(
        runtime: &DocumentRuntime,
        binding: M11HotInlineSidecarBinding,
        projection: &M11BlockQuoteProjectionRoot,
    ) -> Result<Self, M11PublicationError> {
        let envelope =
            M11InlineOverlayEnvelope::from_bullet_list_projection(binding.0.clone(), projection)
                .map_err(M11InlineOverlayTransportError::from)?;
        let descriptor = sidecar_descriptor(
            &envelope,
            u32::try_from(projection.descriptor().storage_page_count()).map_err(|_| {
                M11PublicationError(ErrorInner::Invalid(
                    "bullet-list sidecar node count exceeds transport width",
                ))
            })?,
            u32::try_from(PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES)
                .expect("line-prefix descriptor fits"),
        );
        let inner = M11InlineOverlaySnapshotEncoder::authoritative_bullet_list(
            runtime, binding.0, projection,
        )?;
        Ok(Self { inner, descriptor })
    }

    pub fn authoritative_bullet_list_item(
        runtime: &DocumentRuntime,
        binding: M11HotInlineSidecarBinding,
        projection: &M11BlockQuoteProjectionRoot,
        selected_item_ordinal: u32,
        selected_item_line_ending: M11HotInlineCanonicalLineEnding,
    ) -> Result<Self, M11PublicationError> {
        let envelope = M11InlineOverlayEnvelope::from_bullet_list_item_projection(
            binding.0.clone(),
            projection,
            selected_item_ordinal,
            selected_item_line_ending.into(),
        )
        .map_err(M11InlineOverlayTransportError::from)?;
        let descriptor = sidecar_descriptor(
            &envelope,
            u32::try_from(projection.descriptor().storage_page_count()).map_err(|_| {
                M11PublicationError(ErrorInner::Invalid(
                    "bullet-list item sidecar node count exceeds transport width",
                ))
            })?,
            u32::try_from(PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES)
                .expect("line-prefix descriptor fits"),
        );
        let inner = M11InlineOverlaySnapshotEncoder::authoritative_bullet_list_item(
            runtime,
            binding.0,
            projection,
            selected_item_ordinal,
            selected_item_line_ending.into(),
        )?;
        Ok(Self { inner, descriptor })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn authoritative_ordered_list_item(
        runtime: &DocumentRuntime,
        binding: M11HotInlineSidecarBinding,
        projection: &M11BlockQuoteProjectionRoot,
        selected_item_ordinal: u32,
        selected_item_line_ending: M11HotInlineCanonicalLineEnding,
        opening_marker_start: u32,
        opening_marker_end: u32,
        marker_value: u32,
    ) -> Result<Self, M11PublicationError> {
        let envelope = M11InlineOverlayEnvelope::from_ordered_list_item_projection(
            binding.0.clone(),
            projection,
            selected_item_ordinal,
            selected_item_line_ending.into(),
            opening_marker_start,
            opening_marker_end,
            marker_value,
        )
        .map_err(M11InlineOverlayTransportError::from)?;
        let descriptor = sidecar_descriptor(
            &envelope,
            u32::try_from(projection.descriptor().storage_page_count()).map_err(|_| {
                M11PublicationError(ErrorInner::Invalid(
                    "ordered-list item sidecar node count exceeds transport width",
                ))
            })?,
            u32::try_from(PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES)
                .expect("line-prefix descriptor fits"),
        );
        let inner = M11InlineOverlaySnapshotEncoder::authoritative_ordered_list_item(
            runtime,
            binding.0,
            projection,
            selected_item_ordinal,
            selected_item_line_ending.into(),
            opening_marker_start,
            opening_marker_end,
            marker_value,
        )?;
        Ok(Self { inner, descriptor })
    }

    pub fn unsupported(
        runtime: &DocumentRuntime,
        binding: M11HotInlineSidecarBinding,
        reason: u32,
        metadata: Box<[u8]>,
    ) -> Result<Self, M11PublicationError> {
        let envelope = M11InlineOverlayEnvelope::unsupported(binding.0.clone(), reason, &metadata)
            .map_err(M11InlineOverlayTransportError::from)?;
        let descriptor = sidecar_descriptor(&envelope, 1, 0);
        let inner =
            M11InlineOverlaySnapshotEncoder::unsupported(runtime, binding.0, reason, metadata)?;
        Ok(Self { inner, descriptor })
    }

    #[must_use]
    pub const fn descriptor(&self) -> M11HotInlineSidecarDescriptor {
        self.descriptor
    }

    pub fn begin_frame(&mut self) -> Result<M11HotInlineSidecarFrame, M11PublicationError> {
        Ok(M11HotInlineSidecarFrame {
            kind: M11HotInlineSidecarFrameKind::Begin,
            node_ordinal: None,
            root_stream_digest256: None,
            bytes: self.inner.begin_frame()?,
        })
    }

    pub fn poll(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
    ) -> Result<M11HotInlineSidecarSnapshotPoll, M11PublicationError> {
        match self.inner.poll(runtime, fuel)? {
            M11InlineOverlaySnapshotEncodePoll::Pending { transitions } => {
                Ok(M11HotInlineSidecarSnapshotPoll::Pending { transitions })
            }
            M11InlineOverlaySnapshotEncodePoll::Frame { transitions, bytes } => {
                let metadata = classify_snapshot_frame(&bytes)?;
                if metadata.kind != SnapshotFrameKind::Node {
                    return Err(M11PublicationError(ErrorInner::Invalid(
                        "hot-inline closure emitted a non-Node frame",
                    )));
                }
                Ok(M11HotInlineSidecarSnapshotPoll::Frame {
                    transitions,
                    frame: M11HotInlineSidecarFrame {
                        kind: M11HotInlineSidecarFrameKind::Node,
                        node_ordinal: metadata.node_ordinal,
                        root_stream_digest256: None,
                        bytes,
                    },
                })
            }
            M11InlineOverlaySnapshotEncodePoll::Complete { transitions, bytes } => {
                let metadata = classify_snapshot_frame(&bytes)?;
                if metadata.kind != SnapshotFrameKind::End
                    || metadata.canonical_stream_digest256.is_none()
                {
                    return Err(M11PublicationError(ErrorInner::Invalid(
                        "hot-inline closure emitted an invalid End frame",
                    )));
                }
                Ok(M11HotInlineSidecarSnapshotPoll::Frame {
                    transitions,
                    frame: M11HotInlineSidecarFrame {
                        kind: M11HotInlineSidecarFrameKind::End,
                        node_ordinal: None,
                        root_stream_digest256: metadata.canonical_stream_digest256,
                        bytes,
                    },
                })
            }
        }
    }
}

fn sidecar_descriptor(
    envelope: &M11InlineOverlayEnvelope,
    transferred_node_count: u32,
    ipr2_descriptor_bytes: u32,
) -> M11HotInlineSidecarDescriptor {
    let encoded = envelope.encode();
    let hio1_envelope_digest256 = encoded[M11_INLINE_OVERLAY_ENVELOPE_BYTES - 32..]
        .try_into()
        .expect("HIO1 digest is fixed width");
    let disposition = match *envelope.disposition() {
        M11InlineOverlayDisposition::Authoritative {
            projection_kind: M11InlineOverlayProjectionKind::Inline,
            selected_item_ordinal: _,
            selected_item_line_ending: _,
            ordered_item: _,
            logical_page_count,
            fact_count,
            storage_page_count,
            ordered_commitment256,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
        } => M11HotInlineSidecarDisposition::Authoritative {
            logical_page_count,
            fact_count,
            storage_page_count,
            link_value_entry_count,
            link_value_storage_page_count,
            link_value_encoded_bytes,
            ordered_commitment256,
        },
        M11InlineOverlayDisposition::Authoritative {
            projection_kind: M11InlineOverlayProjectionKind::IndentedCode,
            selected_item_ordinal: _,
            selected_item_line_ending: _,
            ordered_item: _,
            logical_page_count,
            fact_count,
            storage_page_count,
            ordered_commitment256,
            ..
        } => M11HotInlineSidecarDisposition::IndentedCodeAuthoritative {
            logical_page_count,
            line_count: fact_count,
            storage_page_count,
            ordered_commitment256,
        },
        M11InlineOverlayDisposition::Authoritative {
            projection_kind: M11InlineOverlayProjectionKind::BlockQuote,
            selected_item_ordinal: _,
            selected_item_line_ending: _,
            ordered_item: _,
            logical_page_count,
            fact_count,
            storage_page_count,
            ordered_commitment256,
            ..
        } => M11HotInlineSidecarDisposition::BlockQuoteAuthoritative {
            logical_page_count,
            line_count: fact_count,
            storage_page_count,
            ordered_commitment256,
        },
        M11InlineOverlayDisposition::Authoritative {
            projection_kind: M11InlineOverlayProjectionKind::BulletList,
            selected_item_ordinal,
            selected_item_line_ending,
            ordered_item: _,
            logical_page_count,
            fact_count,
            storage_page_count,
            ordered_commitment256,
            ..
        } => M11HotInlineSidecarDisposition::BulletListAuthoritative {
            selected_item_ordinal,
            selected_item_line_ending: selected_item_line_ending.map(Into::into),
            logical_page_count,
            item_count: fact_count,
            storage_page_count,
            ordered_commitment256,
        },
        M11InlineOverlayDisposition::Authoritative {
            projection_kind: M11InlineOverlayProjectionKind::OrderedList,
            selected_item_ordinal: _,
            selected_item_line_ending: _,
            ordered_item: Some(ordered_item),
            logical_page_count,
            fact_count,
            storage_page_count,
            ordered_commitment256,
            ..
        } => M11HotInlineSidecarDisposition::OrderedListAuthoritative {
            selected_item_ordinal: ordered_item.selected_item_ordinal,
            selected_item_line_ending: ordered_item.selected_item_line_ending.into(),
            opening_marker_start: ordered_item.opening_marker_start,
            opening_marker_end: ordered_item.opening_marker_end,
            marker_value: ordered_item.marker_value,
            logical_page_count,
            item_count: fact_count,
            storage_page_count,
            ordered_commitment256,
        },
        M11InlineOverlayDisposition::Authoritative {
            projection_kind: M11InlineOverlayProjectionKind::OrderedList,
            ..
        } => unreachable!("ordered-list HIO1 disposition requires selected item metadata"),
        M11InlineOverlayDisposition::Unsupported {
            reason,
            metadata_commitment256,
        } => M11HotInlineSidecarDisposition::Unsupported {
            reason,
            metadata_commitment256,
        },
    };
    M11HotInlineSidecarDescriptor {
        hio1_encoded_bytes: u32::try_from(M11_INLINE_OVERLAY_ENVELOPE_BYTES).expect("HIO1 fits"),
        ipr2_descriptor_bytes,
        transferred_node_count,
        hio1_envelope_digest256,
        disposition,
    }
}

/// Owned producer stream that can live directly in an endpoint state machine.
pub struct M11OwnedSnapshotStream {
    runtime_identity: StrongIdentity,
    publication: Option<PublishedManifest>,
    closing_root: Option<CommittedArenaRoot>,
    exact_base_publication: Option<PublishedManifest>,
    exact_base_closing_root: Option<CommittedArenaRoot>,
    reference_winner: Option<M11ReferenceWinnerHandle>,
    close_complete: bool,
    traversal_complete: bool,
    cancelled_for_exact_base_restore: bool,
    transferred_canonical_record_count: u64,
    state: CandidateSnapshotEncoderState,
}

impl Drop for M11OwnedSnapshotStream {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            debug_assert!(
                self.close_complete
                    && self.publication.is_none()
                    && self.closing_root.is_none()
                    && self.exact_base_publication.is_none()
                    && self.exact_base_closing_root.is_none()
                    && self.reference_winner.is_none(),
                "M1.1 snapshot stream must fuel-close its runtime-owned publication"
            );
        }
    }
}

impl M11OwnedSnapshotStream {
    /// Number of canonical records physically transferred by this stream.
    ///
    /// For a full snapshot this equals the target count. For an exact-base
    /// stream it excludes reused References and retained SourceFacts pages.
    #[must_use]
    pub const fn transferred_canonical_record_count(&self) -> u64 {
        self.transferred_canonical_record_count
    }

    pub fn descriptor(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<M11CandidateDescriptor, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        descriptor_from_publication(runtime.producer_arena(), self.publication()?)
    }

    pub fn begin_frame(&mut self) -> Result<M11SnapshotFrame, M11PublicationError> {
        if self.cancelled_for_exact_base_restore {
            return Err(M11PublicationError::invalid_state());
        }
        classified_snapshot_frame(self.state.begin_frame()?)
    }

    pub fn poll(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
    ) -> Result<M11OwnedSnapshotPoll, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        if self.cancelled_for_exact_base_restore {
            return Err(M11PublicationError::invalid_state());
        }
        match self.state.poll(runtime.producer_arena(), fuel)? {
            CandidateSnapshotEncodePoll::Pending { transitions } => {
                Ok(M11OwnedSnapshotPoll::Pending { transitions })
            }
            CandidateSnapshotEncodePoll::Frame { transitions, bytes } => {
                Ok(M11OwnedSnapshotPoll::Frame {
                    transitions,
                    frame: classified_snapshot_frame(bytes)?,
                })
            }
            CandidateSnapshotEncodePoll::ReplayRequired { transitions } => {
                Ok(M11OwnedSnapshotPoll::ReplayRequired { transitions })
            }
            CandidateSnapshotEncodePoll::Complete { transitions, bytes } => {
                let frame = classified_snapshot_frame(bytes)?;
                self.traversal_complete = true;
                Ok(M11OwnedSnapshotPoll::Frame { transitions, frame })
            }
        }
    }

    /// Resumes target-node output after the host has completed the exact-base
    /// SourceFacts replay barrier.
    pub fn resume_exact_base_delta(&mut self) -> Result<(), M11PublicationError> {
        if self.cancelled_for_exact_base_restore {
            return Err(M11PublicationError::invalid_state());
        }
        self.state.resume_exact_base_delta().map_err(Into::into)
    }

    /// Terminates an in-flight exact-base target and detaches the still-valid
    /// producer base before the target stream is fuel-closed.
    ///
    /// After this handback the stream is cancellation-only: it cannot emit,
    /// resume, or become a retained target publication.
    pub fn take_exact_base_for_cancel(
        &mut self,
        runtime: &DocumentRuntime,
    ) -> Result<M11RetainedCandidatePublication, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        if self.cancelled_for_exact_base_restore
            || self.close_complete
            || self.closing_root.is_some()
            || self.exact_base_closing_root.is_some()
        {
            return Err(M11PublicationError::invalid_state());
        }
        let publication = self
            .exact_base_publication
            .take()
            .ok_or_else(M11PublicationError::invalid_state)?;
        self.cancelled_for_exact_base_restore = true;
        Ok(M11RetainedCandidatePublication {
            runtime_identity: self.runtime_identity,
            publication: Some(publication),
            closing_root: None,
            reference_winner: self.reference_winner.take(),
            close_complete: false,
        })
    }

    /// Detaches the superseded exact base after the terminal target frame.
    ///
    /// Full streams return `None`. Exact-base target handback remains blocked
    /// until this retained owner has been taken and scheduled for fuel-close.
    pub fn take_superseded_exact_base(
        &mut self,
        runtime: &DocumentRuntime,
    ) -> Result<Option<M11RetainedCandidatePublication>, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        if self.cancelled_for_exact_base_restore
            || !self.traversal_complete
            || self.exact_base_closing_root.is_some()
            || self.close_complete
        {
            return Err(M11PublicationError::invalid_state());
        }
        let Some(publication) = self.exact_base_publication.take() else {
            return Ok(None);
        };
        Ok(Some(M11RetainedCandidatePublication {
            runtime_identity: self.runtime_identity,
            publication: Some(publication),
            closing_root: None,
            reference_winner: self.reference_winner.take(),
            close_complete: false,
        }))
    }

    /// Returns this move-only root as a sealed retained publication.
    ///
    /// Wrong-runtime and premature handback attempts return the intact stream,
    /// so the caller can continue traversal or explicitly fuel-close it.
    pub fn into_retained_publication(
        mut self,
        runtime: &DocumentRuntime,
    ) -> Result<M11RetainedCandidatePublication, M11SnapshotRetentionFailure> {
        if let Err(error) = validate_runtime(self.runtime_identity, runtime) {
            return Err(M11SnapshotRetentionFailure {
                error,
                stream: Box::new(self),
            });
        }
        if !self.traversal_complete
            || self.cancelled_for_exact_base_restore
            || self.publication.is_none()
            || self.closing_root.is_some()
            || self.exact_base_publication.is_some()
            || self.exact_base_closing_root.is_some()
            || self.close_complete
        {
            return Err(M11SnapshotRetentionFailure {
                error: M11PublicationError::invalid_state(),
                stream: Box::new(self),
            });
        }
        let publication = self
            .publication
            .take()
            .expect("validated snapshot stream retains its publication");
        self.close_complete = true;
        Ok(M11RetainedCandidatePublication {
            runtime_identity: self.runtime_identity,
            publication: Some(publication),
            closing_root: None,
            reference_winner: self.reference_winner.take(),
            close_complete: false,
        })
    }

    pub fn begin_close(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        if let Some(winner) = self.reference_winner.as_mut() {
            winner.begin_release()?;
        }
        begin_publication_release(
            runtime.producer_arena_mut(),
            &mut self.publication,
            &mut self.closing_root,
        )?;
        begin_publication_release(
            runtime.producer_arena_mut(),
            &mut self.exact_base_publication,
            &mut self.exact_base_closing_root,
        )
    }

    pub fn poll_close(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<bool, M11PublicationError> {
        validate_runtime(self.runtime_identity, runtime)?;
        if fuel == 0 {
            return Err(M11PublicationError::zero_fuel());
        }
        let mut remaining = fuel;
        if let Some(winner) = self.reference_winner.as_mut() {
            let (consumed, complete) = winner.poll_release(runtime, remaining)?;
            remaining = remaining
                .checked_sub(consumed)
                .ok_or_else(M11PublicationError::invalid_state)?;
            if !complete {
                return Ok(false);
            }
            drop(self.reference_winner.take());
            if remaining == 0 {
                return Ok(false);
            }
        }
        if self.publication.is_some()
            || self.closing_root.is_some()
            || self.exact_base_publication.is_some()
            || self.exact_base_closing_root.is_some()
        {
            return Err(M11PublicationError::invalid_state());
        }
        let complete = runtime
            .producer_arena_mut()
            .poll_reclaim(remaining)
            .complete;
        self.close_complete = complete;
        Ok(complete)
    }

    fn publication(&self) -> Result<&PublishedManifest, M11PublicationError> {
        self.publication
            .as_ref()
            .ok_or_else(M11PublicationError::invalid_state)
    }
}

/// Recoverable move-only handback failure.
pub struct M11SnapshotRetentionFailure {
    error: M11PublicationError,
    stream: Box<M11OwnedSnapshotStream>,
}

impl M11SnapshotRetentionFailure {
    pub fn into_parts(self) -> (M11PublicationError, M11OwnedSnapshotStream) {
        (self.error, *self.stream)
    }
}

impl fmt::Debug for M11SnapshotRetentionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11SnapshotRetentionFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

pub enum M11OwnedSnapshotPoll {
    Pending {
        transitions: usize,
    },
    ReplayRequired {
        transitions: usize,
    },
    Frame {
        transitions: usize,
        frame: M11SnapshotFrame,
    },
}

pub struct M11SnapshotEncoder<'a>(CandidateSnapshotEncoder<'a>);

impl M11SnapshotEncoder<'_> {
    pub fn begin_frame(&mut self) -> Result<Box<[u8]>, M11PublicationError> {
        self.0.begin_frame().map_err(Into::into)
    }

    pub fn poll(&mut self, fuel: usize) -> Result<M11SnapshotPoll, M11PublicationError> {
        match self.0.poll(fuel)? {
            CandidateSnapshotEncodePoll::Pending { transitions } => {
                Ok(M11SnapshotPoll::Pending { transitions })
            }
            CandidateSnapshotEncodePoll::Frame { transitions, bytes } => {
                Ok(M11SnapshotPoll::Node { transitions, bytes })
            }
            CandidateSnapshotEncodePoll::ReplayRequired { .. } => {
                Err(M11PublicationError::invalid_state())
            }
            CandidateSnapshotEncodePoll::Complete { transitions, bytes } => {
                Ok(M11SnapshotPoll::End { transitions, bytes })
            }
        }
    }
}

pub enum M11SnapshotPoll {
    Pending {
        transitions: usize,
    },
    Node {
        transitions: usize,
        bytes: Box<[u8]>,
    },
    End {
        transitions: usize,
        bytes: Box<[u8]>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11Role {
    SourceFacts,
    Green,
    Projection,
    CleanEofOnly,
}

impl M11Role {
    const fn engine(self) -> CandidateRole {
        match self {
            Self::SourceFacts => CandidateRole::SourceFacts,
            Self::Green => CandidateRole::Green,
            Self::Projection => CandidateRole::Projection,
            Self::CleanEofOnly => CandidateRole::CleanEofOnly,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11InstalledCandidate(InstalledCandidateSnapshot);

impl M11InstalledCandidate {
    #[must_use]
    pub const fn source_revision(self) -> u64 {
        self.0.source_revision().get()
    }

    #[must_use]
    pub const fn parse_generation(self) -> u64 {
        self.0.parse_generation().get()
    }
}

/// Independent candidate host. No producer arena handle crosses this type.
pub struct M11CandidateHost(CandidateHostStore);

impl M11CandidateHost {
    pub fn new(
        document: [u8; 16],
        source: SourceVersion,
        syntax_profile: u32,
    ) -> Result<Self, M11PublicationError> {
        Ok(Self(CandidateHostStore::new(
            StrongIdentity::new(document)?,
            source,
            syntax_profile,
            CandidateHostLimits::default(),
        )?))
    }

    pub fn begin_snapshot(&mut self, frame: &[u8]) -> Result<(), M11PublicationError> {
        self.0.begin_snapshot(frame).map_err(Into::into)
    }

    pub fn offer_node(&mut self, frame: &[u8]) -> Result<(), M11PublicationError> {
        self.0.offer_node(frame).map_err(Into::into)
    }

    pub fn finish_snapshot(&mut self, frame: &[u8]) -> Result<(), M11PublicationError> {
        self.0.finish_snapshot(frame).map_err(Into::into)
    }

    pub fn poll_install(&mut self, fuel: usize) -> Result<M11HostInstallPoll, M11PublicationError> {
        let CandidateHostInstallPoll {
            transitions,
            installed,
        } = self.0.poll_install(fuel)?;
        Ok(M11HostInstallPoll {
            transitions,
            installed: installed.map(M11InstalledCandidate),
        })
    }

    #[must_use]
    pub fn installed(&self) -> Option<M11InstalledCandidate> {
        self.0.installed_snapshot().map(M11InstalledCandidate)
    }

    pub fn read_role(
        &self,
        installed: M11InstalledCandidate,
        role: M11Role,
        offset: usize,
        output: &mut [u8],
    ) -> Result<usize, M11PublicationError> {
        self.0
            .read_role_record(installed.0, role.engine(), offset, output)
            .map_err(Into::into)
    }

    pub fn read_role_record(
        &self,
        installed: M11InstalledCandidate,
        role: M11Role,
        ordinal: u64,
        offset: usize,
        output: &mut [u8],
    ) -> Result<usize, M11PublicationError> {
        self.0
            .read_role_record_at(installed.0, role.engine(), ordinal, offset, output)
            .map_err(Into::into)
    }

    pub fn role_record_count(
        &self,
        installed: M11InstalledCandidate,
        role: M11Role,
    ) -> Result<u64, M11PublicationError> {
        self.0
            .role_record_count(installed.0, role.engine())
            .map_err(Into::into)
    }

    pub fn installed_manifest_digest256(
        &self,
        installed: M11InstalledCandidate,
    ) -> Result<[u8; 32], M11PublicationError> {
        self.0
            .installed_manifest_digest256(installed.0)
            .map_err(Into::into)
    }

    pub fn locate_block_point(
        &self,
        installed: M11InstalledCandidate,
        point: M11BlockSequencePoint,
    ) -> Result<Option<M11BlockSequenceLocation>, M11PublicationError> {
        self.0
            .persistent_block_point(installed.0, point)
            .map_err(Into::into)
    }

    pub fn locate_recursive_green_point(
        &self,
        installed: M11InstalledCandidate,
        point: M11RecursiveGreenPoint,
    ) -> Result<Option<M11RecursiveGreenLocation>, M11PublicationError> {
        let Some(outcome) = self
            .0
            .persistent_recursive_green_point(installed.0, point, u64::MAX)
            .map_err(M11PublicationError::from)?
        else {
            return Ok(None);
        };
        match outcome {
            crate::recursive_green::M11RecursiveGreenPointQueryOutcome::Location(location) => {
                Ok(Some(location))
            }
            crate::recursive_green::M11RecursiveGreenPointQueryOutcome::NotFound => Ok(None),
            crate::recursive_green::M11RecursiveGreenPointQueryOutcome::BudgetExceeded(_) => {
                unreachable!("u64::MAX recursive Green query budget cannot be exhausted")
            }
        }
    }

    pub fn abort_snapshot(&mut self) -> Result<bool, M11PublicationError> {
        self.0.abort_snapshot().map_err(Into::into)
    }

    pub fn poll_reclaim(&mut self, fuel: usize) -> Result<bool, M11PublicationError> {
        self.0.poll_reclaim(fuel).map_err(Into::into)
    }

    pub fn begin_close(&mut self) -> Result<(), M11PublicationError> {
        self.0.begin_close().map_err(Into::into)
    }

    pub fn poll_close(&mut self, fuel: usize) -> Result<bool, M11PublicationError> {
        self.0.poll_close(fuel).map_err(Into::into)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostInstallPoll {
    pub transitions: usize,
    pub installed: Option<M11InstalledCandidate>,
}

#[cfg(test)]
mod persistent_projection_adoption_tests {
    use super::*;
    use crate::block_sequence::{
        PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA, PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA,
    };
    use crate::candidate_manifest::{
        decode_manifest, persistent_block_manifest_roles, persistent_recursive_green_manifest_role,
        role_index, CandidateRole, PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA,
    };
    use crate::document::{DocumentRuntimeConfig, RuntimeSourceFactsPoll};
    use crate::storage::ArenaLimits;
    use crate::{SourceBoundaryAffinity, SourceFactsRootLimits};

    const LOGICAL_PAGES: usize = M11_MAX_ROLE_RECORDS + 1;

    fn runtime_with_source(source: &str) -> DocumentRuntime {
        DocumentRuntime::new(
            source,
            DocumentRuntimeConfig {
                arena_limits: ArenaLimits {
                    max_slots: M11_CANDIDATE_ARENA_MAX_SLOTS,
                    max_live_payload_bytes: M11_CANDIDATE_ARENA_MAX_LIVE_PAYLOAD_BYTES,
                    max_children_per_node: M11_MAX_ROLE_RECORDS,
                },
                ..DocumentRuntimeConfig::default()
            },
        )
        .expect("document runtime")
    }

    fn install_source_facts(
        runtime: &mut DocumentRuntime,
        scan_profile: SourceFactsScanProfile,
        parser_profile: ParserProfileId,
    ) {
        runtime
            .begin_source_facts(
                scan_profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("begin SourceFacts");
        loop {
            match runtime
                .poll_source_facts(4096, 64)
                .expect("poll SourceFacts")
            {
                RuntimeSourceFactsPoll::Pending(_)
                | RuntimeSourceFactsPoll::PromotionPending { .. }
                | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
                RuntimeSourceFactsPoll::Complete { .. } => break,
                RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
                | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                    panic!("clean SourceFacts build reported incremental progress")
                }
            }
        }
    }

    fn build_inline_root(
        runtime: &mut DocumentRuntime,
        source: SourceVersion,
        parser_profile: ParserProfileId,
    ) -> M11InlineProjectionRoot {
        let lease = runtime.snapshot_current_source().expect("source lease");
        let mut build =
            M11InlineProjectionBuild::new(runtime, lease, 0..source.byte_len(), parser_profile)
                .expect("inline build");
        for ordinal in 0..LOGICAL_PAGES {
            let start = u32::try_from(ordinal * 6).expect("test coordinate");
            if ordinal == 0 {
                let fact = M11InlineProjectionFact::new(
                    M11InlineProjectionKind::DirectLink,
                    0,
                    start..start + 6,
                    start + 1..start + 2,
                )
                .expect("direct-link fact");
                let value = M11InlineLinkValue::new(0, start + 4..start + 5, None, "d", None)
                    .expect("direct-link companion value");
                build
                    .offer_page_with_link_values(&[fact], &[value])
                    .expect("paired logical page");
            } else {
                let fact = M11InlineProjectionFact::new(
                    M11InlineProjectionKind::Strong,
                    0,
                    start..start + 5,
                    start + 2..start + 3,
                )
                .expect("inline fact");
                build.offer_page(&[fact]).expect("logical page");
            }
            loop {
                match build.poll(runtime, 32).expect("poll logical page").status() {
                    M11InlineProjectionBuildStatus::NeedsPage => break,
                    M11InlineProjectionBuildStatus::Pending => {}
                    M11InlineProjectionBuildStatus::Complete
                    | M11InlineProjectionBuildStatus::Cancelled => {
                        panic!("inline build ended before input")
                    }
                }
            }
        }
        build.finish_input().expect("finish inline input");
        loop {
            match build
                .poll(runtime, 32)
                .expect("finish inline root")
                .status()
            {
                M11InlineProjectionBuildStatus::Pending => {}
                M11InlineProjectionBuildStatus::Complete => {
                    return build.take_root().expect("persistent inline root")
                }
                M11InlineProjectionBuildStatus::NeedsPage
                | M11InlineProjectionBuildStatus::Cancelled => {
                    panic!("finished inline build returned the wrong state")
                }
            }
        }
    }

    fn build_block_root(
        runtime: &mut DocumentRuntime,
        entries: impl IntoIterator<Item = M11BlockSequenceEntry>,
    ) -> M11BlockSequenceRoot {
        let lease = runtime.snapshot_current_source().expect("source lease");
        let mut build = M11BlockSequenceBuild::new(runtime, lease).expect("block build");
        for entry in entries {
            build.offer_entry(entry).expect("block entry");
            loop {
                match build.poll(runtime, 32).expect("poll block entry").status() {
                    M11BlockSequenceBuildStatus::NeedsInput => break,
                    M11BlockSequenceBuildStatus::Pending => {}
                    M11BlockSequenceBuildStatus::Complete
                    | M11BlockSequenceBuildStatus::Cancelled => {
                        panic!("block build ended before input")
                    }
                }
            }
        }
        build.finish_input().expect("finish block input");
        loop {
            match build.poll(runtime, 32).expect("finish block root").status() {
                M11BlockSequenceBuildStatus::Pending => {}
                M11BlockSequenceBuildStatus::Complete => {
                    return build.take_root().expect("persistent block root")
                }
                M11BlockSequenceBuildStatus::NeedsInput
                | M11BlockSequenceBuildStatus::Cancelled => {
                    panic!("finished block build returned the wrong state")
                }
            }
        }
    }

    fn offer_recursive_green_event(
        runtime: &mut DocumentRuntime,
        build: &mut M11RecursiveGreenBuild,
        event: M11RecursiveGreenEvent,
    ) {
        build.offer_event(event).expect("recursive Green event");
        loop {
            match build
                .poll(runtime, 32)
                .expect("poll recursive Green event")
                .status()
            {
                M11RecursiveGreenBuildStatus::NeedsInput => break,
                M11RecursiveGreenBuildStatus::Pending => {}
                M11RecursiveGreenBuildStatus::Complete
                | M11RecursiveGreenBuildStatus::Cancelled => {
                    panic!("recursive Green build ended before input")
                }
            }
        }
    }

    fn build_recursive_green_root(runtime: &mut DocumentRuntime) -> M11RecursiveGreenRoot {
        let source = runtime.current_source_version().expect("current source");
        let frame = M11RecursiveGreenFrameId::new(1).expect("frame");
        let kind = M11RecursiveGreenKind::new(1).expect("kind");
        let physical = M11RecursiveGreenSourceMetric::new(
            u64::try_from(source.byte_len()).expect("source bytes"),
            u64::try_from(source.utf16_len()).expect("source UTF-16"),
        )
        .expect("source metric");
        let lease = runtime
            .snapshot_current_source()
            .expect("Green source lease");
        let mut build = M11RecursiveGreenBuild::new(runtime, lease).expect("Green build");
        offer_recursive_green_event(
            runtime,
            &mut build,
            M11RecursiveGreenEvent::Enter { frame, kind },
        );
        offer_recursive_green_event(
            runtime,
            &mut build,
            M11RecursiveGreenEvent::Coverage {
                physical,
                owner_depth: 0,
                part: M11RecursiveGreenCoveragePart::Content,
                logical: M11RecursiveGreenLogicalAction::CanonicalText,
            },
        );
        offer_recursive_green_event(
            runtime,
            &mut build,
            M11RecursiveGreenEvent::Exit {
                frame,
                final_kind: kind,
                close: None,
                last_line_blank: false,
                child: M11RecursiveGreenClosedChild::default(),
            },
        );
        build.finish_input().expect("finish Green input");
        loop {
            match build
                .poll(runtime, 32)
                .expect("finish recursive Green root")
                .status()
            {
                M11RecursiveGreenBuildStatus::Pending => {}
                M11RecursiveGreenBuildStatus::Complete => {
                    return build.take_root().expect("recursive Green root")
                }
                M11RecursiveGreenBuildStatus::NeedsInput
                | M11RecursiveGreenBuildStatus::Cancelled => {
                    panic!("finished recursive Green build returned the wrong state")
                }
            }
        }
    }

    fn build_reference_journal_root(
        runtime: &mut DocumentRuntime,
        source: SourceVersion,
    ) -> M11ReferenceJournalRoot {
        let mut journal = M11ReferenceJournal::new(runtime, source, 1).expect("reference journal");
        journal
            .offer_occurrence(
                runtime,
                M11ReferenceJournalOccurrence::new(
                    M11ReferenceJournalRange::new(0..5, 0..5),
                    M11ReferenceJournalRange::new(0..1, 0..1),
                    M11ReferenceJournalRange::new(2..4, 2..4),
                    None,
                    &b"a"[..],
                    &b"ph"[..],
                    None,
                ),
            )
            .expect("journal reference");
        loop {
            let poll = journal.poll(runtime, 64).expect("poll journal input");
            if poll.status() == M11ReferenceJournalStatus::NeedsInput {
                break;
            }
            assert_eq!(poll.status(), M11ReferenceJournalStatus::Pending);
        }
        journal.finish_input(runtime).expect("finish journal input");
        loop {
            let poll = journal.poll(runtime, 64).expect("finish journal");
            if poll.status() == M11ReferenceJournalStatus::Complete {
                return journal.take_root().expect("reference journal root");
            }
            assert_eq!(poll.status(), M11ReferenceJournalStatus::Pending);
        }
    }

    fn retain_publication(
        runtime: &DocumentRuntime,
        publication: M11CandidatePublication,
    ) -> M11RetainedCandidatePublication {
        let mut stream = Box::new(publication)
            .into_snapshot_stream(runtime)
            .expect("snapshot stream");
        assert_eq!(
            stream.begin_frame().expect("snapshot Begin").kind,
            M11SnapshotFrameKind::Begin
        );
        while !stream.traversal_complete {
            match stream.poll(runtime, 256).expect("snapshot traversal") {
                M11OwnedSnapshotPoll::Pending { .. } | M11OwnedSnapshotPoll::Frame { .. } => {}
                M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                    panic!("full snapshot requested exact-base replay")
                }
            }
        }
        stream
            .into_retained_publication(runtime)
            .expect("retained publication")
    }

    fn retained_reference_base(
        runtime: &mut DocumentRuntime,
        source: SourceVersion,
        scan_profile: SourceFactsScanProfile,
        document: [u8; 16],
        publication: [u8; 16],
    ) -> M11RetainedCandidatePublication {
        let mut build = M11CandidateBuild::new_with_persistent_source_facts(
            runtime,
            document,
            publication,
            source,
            1,
            1,
            scan_profile,
            M11RoleRecords::persistent(
                Box::<[u8]>::from(&b"base-green"[..]),
                Box::<[u8]>::from(&b"base-projection"[..]),
            )
            .expect("base role records"),
        )
        .expect("base candidate");
        build
            .offer_reference(
                runtime,
                M11ReferenceRecord::new(
                    M11ReferenceRange::new(0..5, 0..5),
                    M11ReferenceRange::new(0..1, 0..1),
                    M11ReferenceRange::new(2..4, 2..4),
                    None,
                    Box::<[u8]>::from(&b"a"[..]),
                    Box::<[u8]>::from(&b"ph"[..]),
                    None,
                ),
            )
            .expect("base reference");
        while !build.references_idle() {
            assert!(matches!(
                build.poll(runtime, 64).expect("drain base reference"),
                M11CandidateBuildPoll::Pending { .. }
            ));
        }
        build
            .finish_references(runtime)
            .expect("finish base references");
        while matches!(
            build.poll(runtime, 256).expect("publish base"),
            M11CandidateBuildPoll::Pending { .. }
        ) {}
        retain_publication(runtime, build.into_publication().expect("base publication"))
    }

    fn release_block_root(runtime: &mut DocumentRuntime, root: &mut M11BlockSequenceRoot) {
        root.begin_release(runtime).expect("release block root");
        while !root
            .poll_release(runtime, 64)
            .expect("poll block root release")
            .complete()
        {}
    }

    fn close_retained(
        runtime: &mut DocumentRuntime,
        retained: &mut M11RetainedCandidatePublication,
    ) {
        retained.begin_close(runtime).expect("close retained");
        while !retained
            .poll_close(runtime, 64)
            .expect("poll retained close")
        {}
    }

    fn drain_arena_reclaim(runtime: &mut DocumentRuntime) {
        while runtime.arena_metrics().pending_build_aborts != 0
            || runtime.arena_metrics().pending_reclaims != 0
        {
            runtime.producer_arena_mut().poll_reclaim(64);
        }
    }

    fn ready_reference_index_ptr(
        owner: &M11RetainedCandidatePublication,
    ) -> *const ReferenceWinnerIndex {
        let winner = owner
            .reference_winner
            .as_ref()
            .expect("retained reference-winner owner");
        match &winner.state {
            M11ReferenceWinnerState::Ready(index) => {
                assert_eq!(
                    Arc::strong_count(index),
                    1,
                    "winner has one move-only owner"
                );
                Arc::as_ptr(index)
            }
            state => panic!("reference winner was not ready: {state:?}"),
        }
    }

    fn ready_stream_reference_index_ptr(
        stream: &M11OwnedSnapshotStream,
    ) -> *const ReferenceWinnerIndex {
        let winner = stream
            .reference_winner
            .as_ref()
            .expect("stream reference-winner owner");
        match &winner.state {
            M11ReferenceWinnerState::Ready(index) => {
                assert_eq!(
                    Arc::strong_count(index),
                    1,
                    "winner has one move-only owner"
                );
                Arc::as_ptr(index)
            }
            state => panic!("stream reference winner was not ready: {state:?}"),
        }
    }

    fn complete_incremental_source_facts(
        runtime: &mut DocumentRuntime,
    ) -> Box<PersistentSourceFactsDeltaWitness> {
        loop {
            match runtime
                .poll_source_facts(19, 5)
                .expect("poll incremental SourceFacts")
            {
                RuntimeSourceFactsPoll::Pending(_)
                | RuntimeSourceFactsPoll::PromotionPending { .. }
                | RuntimeSourceFactsPoll::IncrementalScanComplete { .. } => {}
                RuntimeSourceFactsPoll::IncrementalComplete { witness, .. } => return witness,
                RuntimeSourceFactsPoll::ScanComplete { .. }
                | RuntimeSourceFactsPoll::Complete { .. } => {
                    panic!("incremental SourceFacts reported clean progress")
                }
            }
        }
    }

    struct ExactReferenceTransferFixture {
        runtime: DocumentRuntime,
        base: M11RetainedCandidatePublication,
        target: M11CandidatePublication,
        witness: Box<PersistentSourceFactsDeltaWitness>,
        target_source: SourceVersion,
        baseline_reserved_external_payload_bytes: usize,
        indexed_reserved_external_payload_bytes: usize,
    }

    fn exact_reference_transfer_fixture() -> ExactReferenceTransferFixture {
        let source_text = format!(
            "abcde\n{}",
            (0..128)
                .map(|ordinal| format!("ordinary prose line {ordinal:04} {}\n", "x".repeat(40)))
                .collect::<String>()
        );
        let mut runtime = runtime_with_source(&source_text);
        let base_source = runtime.current_source_version().expect("base source");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(4).expect("dense scan profile");
        install_source_facts(&mut runtime, scan_profile, parser_profile);
        let mut base = retained_reference_base(
            &mut runtime,
            base_source,
            scan_profile,
            [0xb1; 16],
            [0xb2; 16],
        );
        let baseline_reserved_external_payload_bytes =
            runtime.arena_metrics().reserved_external_payload_bytes;
        loop {
            let poll = base
                .poll_reference_resolver(&mut runtime, 1)
                .expect("build base reference resolver");
            assert!(poll.transitions() <= 1);
            if poll.ready() {
                break;
            }
        }
        assert!(
            runtime.arena_metrics().reserved_external_payload_bytes
                > baseline_reserved_external_payload_bytes
        );
        let indexed_reserved_external_payload_bytes =
            runtime.arena_metrics().reserved_external_payload_bytes;

        let line_prefix = "ordinary prose line 0064 ";
        let edit_start =
            source_text.find(line_prefix).expect("middle fixture line") + line_prefix.len() + 20;
        let target_source = runtime
            .apply_edit(base_source, edit_start..edit_start + 1, "z")
            .expect("apply definition-free edit")
            .source()
            .current();
        runtime
            .begin_incremental_source_facts(
                scan_profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan incremental SourceFacts");
        let witness = complete_incremental_source_facts(&mut runtime);
        let mut target_build =
            M11CandidateBuild::new_with_persistent_source_facts_reusing_references(
                &mut runtime,
                [0xb1; 16],
                [0xb3; 16],
                target_source,
                2,
                1,
                scan_profile,
                M11RoleRecords::persistent(
                    Box::<[u8]>::from(&b"target-green"[..]),
                    Box::<[u8]>::from(&b"target-projection"[..]),
                )
                .expect("target role records"),
                &base,
            )
            .expect("target reusing References");
        while matches!(
            target_build
                .poll(&mut runtime, 256)
                .expect("publish exact target"),
            M11CandidateBuildPoll::Pending { .. }
        ) {}
        let target = target_build
            .into_publication()
            .expect("exact target publication");
        assert_eq!(
            runtime.arena_metrics().reserved_external_payload_bytes,
            indexed_reserved_external_payload_bytes,
            "target construction must not disturb the winner reservation"
        );

        ExactReferenceTransferFixture {
            runtime,
            base,
            target,
            witness,
            target_source,
            baseline_reserved_external_payload_bytes,
            indexed_reserved_external_payload_bytes,
        }
    }

    fn complete_exact_stream(runtime: &DocumentRuntime, stream: &mut M11OwnedSnapshotStream) {
        assert_eq!(
            stream.begin_frame().expect("exact snapshot Begin").kind,
            M11SnapshotFrameKind::Begin
        );
        while !stream.traversal_complete {
            match stream.poll(runtime, 256).expect("poll exact snapshot") {
                M11OwnedSnapshotPoll::Pending { .. } | M11OwnedSnapshotPoll::Frame { .. } => {}
                M11OwnedSnapshotPoll::ReplayRequired { .. } => stream
                    .resume_exact_base_delta()
                    .expect("resume exact-base replay barrier"),
            }
        }
    }

    fn close_stream(runtime: &mut DocumentRuntime, stream: &mut M11OwnedSnapshotStream) {
        stream.begin_close(runtime).expect("begin stream close");
        while !stream.poll_close(runtime, 64).expect("poll stream close") {}
    }

    #[test]
    fn retained_reference_resolver_is_fuelled_bounded_and_releases_its_reservation() {
        let mut runtime = runtime_with_source("abcde");
        let source = runtime.current_source_version().expect("current source");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        install_source_facts(&mut runtime, scan_profile, parser_profile);

        let mut retained =
            retained_reference_base(&mut runtime, source, scan_profile, [0xa1; 16], [0xa2; 16]);
        let baseline_reserved = runtime.arena_metrics().reserved_external_payload_bytes;

        loop {
            let polled = retained
                .poll_reference_resolver(&mut runtime, 1)
                .expect("poll reference resolver");
            assert!(polled.transitions() <= 1);
            if polled.ready() {
                assert_eq!(polled.occurrence_count(), 1);
                assert_eq!(polled.indexed_occurrences(), 1);
                assert_eq!(polled.unique_label_count(), 1);
                break;
            }
            assert_eq!(polled.transitions(), 1);
        }
        assert!(runtime.arena_metrics().reserved_external_payload_bytes > baseline_reserved);

        let resolver = retained
            .reference_resolver(&runtime)
            .expect("reference resolver authority")
            .expect("ready reference resolver");
        let M11ReferenceResolution::Resolved(resolved) = resolver
            .resolve(&runtime, "a", usize::MAX)
            .expect("resolve reference")
        else {
            panic!("defined reference must resolve");
        };
        assert_eq!(resolved.definition_ordinal(), 0);
        assert_eq!(resolved.destination_source(), &(2..4));
        assert_eq!(resolved.title_source(), None);
        assert_eq!(resolved.cooked_destination(), "ph");
        assert_eq!(resolved.cooked_title(), None);
        assert_eq!(
            resolver
                .resolve(&runtime, "a", 1)
                .expect("bounded reference lookup"),
            M11ReferenceResolution::ValueTooLarge,
            "an existing but unrepresentable winner must not masquerade as missing"
        );
        assert_eq!(
            resolver
                .resolve(&runtime, "missing", usize::MAX)
                .expect("missing reference lookup"),
            M11ReferenceResolution::Missing
        );
        drop(resolver);

        close_retained(&mut runtime, &mut retained);
        assert_eq!(
            runtime.arena_metrics().reserved_external_payload_bytes,
            baseline_reserved
        );
        runtime.begin_close().expect("begin runtime close");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    #[test]
    fn exact_base_delivery_moves_the_ready_reference_index_without_rebuilding() {
        let ExactReferenceTransferFixture {
            mut runtime,
            base,
            target,
            witness,
            target_source,
            baseline_reserved_external_payload_bytes,
            indexed_reserved_external_payload_bytes: indexed_reserved,
        } = exact_reference_transfer_fixture();
        let ready_index = ready_reference_index_ptr(&base);

        let mut stream = Box::new(target)
            .into_exact_base_snapshot_stream(&mut runtime, Box::new(base), witness)
            .expect("exact-base stream");
        assert_eq!(ready_stream_reference_index_ptr(&stream), ready_index);
        assert_eq!(
            runtime.arena_metrics().reserved_external_payload_bytes,
            indexed_reserved,
            "moving the ready index into the stream must not reserve again"
        );

        complete_exact_stream(&runtime, &mut stream);
        let mut superseded = stream
            .take_superseded_exact_base(&runtime)
            .expect("detach superseded base")
            .expect("exact stream owns its base");
        assert_eq!(ready_reference_index_ptr(&superseded), ready_index);
        let mut delivered = stream
            .into_retained_publication(&runtime)
            .expect("retain delivered target");
        assert!(delivered
            .reference_resolver(&runtime)
            .expect("target resolver state")
            .is_none());
        delivered
            .adopt_exact_base_reference_resolver(&mut superseded)
            .expect("move ready resolver into delivered target");
        assert_eq!(ready_reference_index_ptr(&delivered), ready_index);
        assert!(superseded.reference_winner.is_none());
        let ready_poll = delivered
            .poll_reference_resolver(&mut runtime, 1)
            .expect("poll already-ready resolver");
        assert!(ready_poll.ready());
        assert_eq!(ready_poll.transitions(), 0, "ready index must not rebuild");
        assert_eq!(
            runtime.arena_metrics().reserved_external_payload_bytes,
            indexed_reserved
        );

        let resolver = delivered
            .reference_resolver(&runtime)
            .expect("delivered resolver authority")
            .expect("delivered ready resolver");
        let M11ReferenceResolution::Resolved(resolved) = resolver
            .resolve(&runtime, "a", usize::MAX)
            .expect("resolve delivered reference")
        else {
            panic!("defined delivered reference must resolve");
        };
        assert_eq!(resolved.cooked_destination(), "ph");
        drop(resolver);

        assert!(runtime
            .commit_persistent_source_facts_delta(target_source)
            .expect("commit delivered SourceFacts target"));
        close_retained(&mut runtime, &mut superseded);
        assert_eq!(
            runtime.arena_metrics().reserved_external_payload_bytes,
            indexed_reserved,
            "closing the superseded non-owner must not release the index"
        );
        close_retained(&mut runtime, &mut delivered);
        assert_eq!(
            runtime.arena_metrics().reserved_external_payload_bytes,
            baseline_reserved_external_payload_bytes,
            "the delivered owner releases the one transferred reservation"
        );
        runtime.begin_close().expect("begin runtime close");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    #[test]
    fn exact_base_cancellation_restores_the_ready_index_and_releases_it_once() {
        let ExactReferenceTransferFixture {
            mut runtime,
            base,
            target,
            witness,
            target_source: _,
            baseline_reserved_external_payload_bytes,
            indexed_reserved_external_payload_bytes: indexed_reserved,
        } = exact_reference_transfer_fixture();
        let ready_index = ready_reference_index_ptr(&base);
        let mut stream = Box::new(target)
            .into_exact_base_snapshot_stream(&mut runtime, Box::new(base), witness)
            .expect("exact-base stream");
        assert_eq!(ready_stream_reference_index_ptr(&stream), ready_index);

        let mut restored = stream
            .take_exact_base_for_cancel(&runtime)
            .expect("restore exact base from cancelled stream");
        assert_eq!(ready_reference_index_ptr(&restored), ready_index);
        assert!(stream.reference_winner.is_none());
        close_stream(&mut runtime, &mut stream);
        assert_eq!(
            runtime.arena_metrics().reserved_external_payload_bytes,
            indexed_reserved,
            "closing the cancelled target must not release the restored index"
        );

        let ready_poll = restored
            .poll_reference_resolver(&mut runtime, 1)
            .expect("poll restored resolver");
        assert!(ready_poll.ready());
        assert_eq!(
            ready_poll.transitions(),
            0,
            "restored index must not rebuild"
        );
        close_retained(&mut runtime, &mut restored);
        assert_eq!(
            runtime.arena_metrics().reserved_external_payload_bytes,
            baseline_reserved_external_payload_bytes,
            "the restored owner releases the one reservation"
        );
        runtime.begin_close().expect("begin runtime close");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    #[test]
    fn mixed_projection_adopts_more_than_flat_fanout_and_survives_host_lifecycle() {
        let source_text = format!("[x](d){}", "**x** ".repeat(LOGICAL_PAGES - 1));
        let mut runtime = runtime_with_source(&source_text);
        let source = runtime.current_source_version().expect("current source");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        install_source_facts(&mut runtime, scan_profile, parser_profile);
        let mut inline_root = build_inline_root(&mut runtime, source, parser_profile);
        assert_eq!(
            inline_root.descriptor().logical_page_count(),
            LOGICAL_PAGES as u64
        );

        let document = [0x31; 16];
        assert_ne!(runtime.producer_identity().0, document);
        let records = M11RoleRecords::persistent(
            Box::<[u8]>::from(&b"green"[..]),
            Box::<[u8]>::from(&b"structural"[..]),
        )
        .expect("mixed structural records");
        let mut candidate =
            M11CandidateBuild::new_with_persistent_source_facts_and_inline_projection(
                &mut runtime,
                document,
                [0x52; 16],
                source,
                1,
                1,
                scan_profile,
                records,
                &inline_root,
            )
            .expect("persistent Projection candidate");

        inline_root
            .begin_release(&mut runtime)
            .expect("release original inline root");
        while !inline_root
            .poll_release(&mut runtime, 64)
            .expect("poll original inline release")
            .complete()
        {}

        candidate
            .finish_references(&runtime)
            .expect("finish references");
        while let M11CandidateBuildPoll::Pending { .. } =
            candidate.poll(&mut runtime, 256).expect("candidate poll")
        {}
        let publication = candidate.into_publication().expect("publication");
        let mut stream = Box::new(publication)
            .into_snapshot_stream(&runtime)
            .expect("snapshot stream");
        let producer_manifest_digest = stream
            .descriptor(&runtime)
            .expect("producer descriptor")
            .manifest_digest256;
        assert_eq!(
            stream.transferred_canonical_record_count(),
            LOGICAL_PAGES as u64 + 5
        );

        let mut host = M11CandidateHost::new(document, source, 1).expect("candidate host");
        let begin = stream.begin_frame().expect("snapshot Begin");
        host.begin_snapshot(&begin.bytes).expect("host Begin");
        loop {
            match stream.poll(&runtime, 256).expect("snapshot poll") {
                M11OwnedSnapshotPoll::Pending { .. } => {}
                M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                    panic!("full snapshot requested exact-base replay")
                }
                M11OwnedSnapshotPoll::Frame { frame, .. } => match frame.kind {
                    M11SnapshotFrameKind::Node => {
                        host.offer_node(&frame.bytes).expect("host Node");
                    }
                    M11SnapshotFrameKind::End => {
                        host.finish_snapshot(&frame.bytes).expect("host End");
                        break;
                    }
                    M11SnapshotFrameKind::Begin
                    | M11SnapshotFrameKind::SourceFactsReplacementPage
                    | M11SnapshotFrameKind::BlockSequenceReplacementPage
                    | M11SnapshotFrameKind::RecursiveGreenReplacementPage => {
                        panic!("full snapshot emitted the wrong frame")
                    }
                },
            }
        }
        let installed = loop {
            let poll = host.poll_install(256).expect("host install");
            if let Some(installed) = poll.installed {
                break installed;
            }
        };
        assert_eq!(
            host.role_record_count(installed, M11Role::Projection)
                .expect("Projection count"),
            LOGICAL_PAGES as u64 + 2
        );
        assert_eq!(
            host.installed_manifest_digest256(installed)
                .expect("host manifest commitment"),
            producer_manifest_digest
        );
        let inline_descriptor = host
            .0
            .persistent_inline_projection_descriptor(installed.0)
            .expect("persistent inline descriptor")
            .expect("persistent inline role");
        assert_eq!(inline_descriptor.link_value_entry_count, 1);
        assert_eq!(inline_descriptor.link_value_encoded_bytes, 49);
        assert!(inline_descriptor.link_value_storage_page_count > 0);
        let mut link_values =
            vec![0_u8; usize::try_from(inline_descriptor.link_value_encoded_bytes).expect("size")];
        let link_receipt = host
            .0
            .copy_persistent_inline_link_values(installed.0, &mut link_values)
            .expect("copy installed link values")
            .expect("persistent inline role");
        assert_eq!(link_receipt.entry_count, 1);
        assert!(link_receipt.tree_nodes_visited > 0);
        assert_eq!(&link_values[..16], b"FLKIV001\x01\0\0\0\x01\0\0\0");
        assert_eq!(&link_values[48..], b"d");
        let mut output = [0_u8; M11_PARSER_PAGE_MAX_RECORD_BYTES];
        let structural = host
            .read_role_record(installed, M11Role::Projection, 0, 0, &mut output)
            .expect("structural Projection record");
        assert_eq!(&output[..structural], b"structural");
        let last = host
            .read_role_record(
                installed,
                M11Role::Projection,
                LOGICAL_PAGES as u64,
                0,
                &mut output,
            )
            .expect("last persistent Projection page");
        assert!(last > 4);
        assert_eq!(&output[..4], b"IFP2");

        stream.begin_close(&mut runtime).expect("close stream");
        while !stream
            .poll_close(&mut runtime, 256)
            .expect("poll stream close")
        {}
        host.begin_close().expect("close host");
        while !host.poll_close(256).expect("poll host close") {}
        runtime.begin_close().expect("close runtime");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    #[test]
    fn paired_block_roles_share_one_root_and_host_queries_one_bounded_leaf() {
        let mut runtime = runtime_with_source("é\nx");
        let source = runtime.current_source_version().expect("current source");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        install_source_facts(&mut runtime, scan_profile, parser_profile);
        let record = |bytes: &[u8]| M11BlockRoleRecord::new(bytes).expect("block record");
        let mut block_root = build_block_root(
            &mut runtime,
            [
                M11BlockSequenceEntry::paragraph(
                    "é".len(),
                    "é".encode_utf16().count(),
                    2,
                    record(b"g0"),
                    record(b"p0"),
                )
                .expect("first paragraph"),
                M11BlockSequenceEntry::blank(1, 1).expect("blank"),
                M11BlockSequenceEntry::paragraph(1, 1, 0, record(b"g1"), record(b"p1"))
                    .expect("second paragraph"),
            ],
        );
        let canonical_root = block_root.tree_root_id_for_test().expect("block tree");
        let document = [0x71; 16];
        let mut candidate = M11CandidateBuild::new_with_persistent_source_facts_and_blocks(
            &mut runtime,
            document,
            [0x72; 16],
            source,
            1,
            1,
            scan_profile,
            &block_root,
        )
        .expect("persistent block candidate");
        block_root
            .begin_release(&mut runtime)
            .expect("release original block root");
        while !block_root
            .poll_release(&mut runtime, 64)
            .expect("poll original block release")
            .complete()
        {}

        candidate
            .finish_references(&runtime)
            .expect("finish references");
        while let M11CandidateBuildPoll::Pending { .. } =
            candidate.poll(&mut runtime, 256).expect("candidate poll")
        {}
        let publication = candidate.into_publication().expect("publication");
        let published = publication
            .publication
            .as_ref()
            .expect("published manifest");
        let descriptor = decode_manifest(
            runtime.producer_arena(),
            published.root_id(),
            published.authority(),
        )
        .expect("paired block manifest");
        let roles = persistent_block_manifest_roles(
            runtime.producer_arena(),
            &descriptor,
            published.authority(),
        )
        .expect("paired block roles");
        assert_eq!(roles.root, Some(canonical_root));
        assert_eq!(roles.claim.entry_count(), 3);
        assert_eq!(roles.claim.reference_definition_count(), 2);
        assert_eq!(
            descriptor.metadata[role_index(CandidateRole::Green)].record_count,
            3
        );
        assert_eq!(
            descriptor.metadata[role_index(CandidateRole::Projection)].record_count,
            3
        );
        assert_ne!(
            roles.green.commitment256(),
            roles.projection.commitment256()
        );

        let mut stream = Box::new(publication)
            .into_snapshot_stream(&runtime)
            .expect("snapshot stream");
        assert!(
            stream.transferred_canonical_record_count() >= 7,
            "both logical block lanes plus clean EOF must be counted"
        );
        let mut host = M11CandidateHost::new(document, source, 1).expect("candidate host");
        let begin = stream.begin_frame().expect("snapshot Begin");
        host.begin_snapshot(&begin.bytes).expect("host Begin");
        loop {
            match stream.poll(&runtime, 256).expect("snapshot poll") {
                M11OwnedSnapshotPoll::Pending { .. } => {}
                M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                    panic!("full snapshot requested exact-base replay")
                }
                M11OwnedSnapshotPoll::Frame { frame, .. } => match frame.kind {
                    M11SnapshotFrameKind::Node => host.offer_node(&frame.bytes).expect("host Node"),
                    M11SnapshotFrameKind::End => {
                        host.finish_snapshot(&frame.bytes).expect("host End");
                        break;
                    }
                    M11SnapshotFrameKind::Begin
                    | M11SnapshotFrameKind::SourceFactsReplacementPage
                    | M11SnapshotFrameKind::BlockSequenceReplacementPage
                    | M11SnapshotFrameKind::RecursiveGreenReplacementPage => {
                        panic!("full snapshot emitted the wrong frame")
                    }
                },
            }
        }
        let installed = loop {
            let poll = host.poll_install(256).expect("host install");
            if let Some(installed) = poll.installed {
                break installed;
            }
        };
        let installed_descriptor = host
            .0
            .persistent_block_descriptor(installed.0)
            .expect("installed block descriptor")
            .expect("persistent blocks");
        assert_eq!(installed_descriptor.source_bytes, 4);
        assert_eq!(installed_descriptor.source_utf16, 3);
        assert_eq!(installed_descriptor.entry_count, 3);
        assert_eq!(installed_descriptor.reference_definition_count, 2);
        assert_eq!(installed_descriptor.storage_page_count, 1);
        assert_eq!(installed_descriptor.tree_height, 1);
        assert_eq!(installed_descriptor.maximum_tree_nodes_visited, 2);

        let second = host
            .locate_block_point(
                installed,
                M11BlockSequencePoint::new(3, 2, SourceBoundaryAffinity::After),
            )
            .expect("second paragraph query")
            .expect("second paragraph");
        assert_eq!(second.byte_range(), 3..4);
        assert_eq!(second.utf16_range(), 2..3);
        assert_eq!(second.entry().green().expect("Green").as_bytes(), b"g1");
        assert_eq!(
            second.entry().projection().expect("Projection").as_bytes(),
            b"p1"
        );
        assert!(second.receipt().entries_scanned() <= 3);
        assert!(
            second.receipt().entries_authenticated()
                > u64::from(second.receipt().entries_scanned()),
            "full packed-page authentication and prefix selection are distinct work"
        );
        assert!(second.receipt().payload_bytes_inspected() > 0);
        let mismatch = host
            .locate_block_point(
                installed,
                M11BlockSequencePoint::new(3, 0, SourceBoundaryAffinity::After),
            )
            .expect_err("mismatched coordinates");
        assert!(matches!(
            mismatch.0,
            ErrorInner::Host(CandidateHostError::BlockSequence(
                M11BlockSequenceError::InvalidPoint
            ))
        ));

        stream.begin_close(&mut runtime).expect("close stream");
        while !stream
            .poll_close(&mut runtime, 256)
            .expect("poll stream close")
        {}
        host.begin_close().expect("close host");
        while !host.poll_close(256).expect("poll host close") {}
        runtime.begin_close().expect("close runtime");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    #[test]
    fn exact_crop_reuses_references_with_unbounded_logical_projection_pages() {
        let source_text = format!("[x](d){}", "**x** ".repeat(LOGICAL_PAGES - 1));
        let mut runtime = runtime_with_source(&source_text);
        let source = runtime.current_source_version().expect("current source");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        install_source_facts(&mut runtime, scan_profile, parser_profile);
        let mut inline_root = build_inline_root(&mut runtime, source, parser_profile);
        assert!(inline_root.descriptor().logical_page_count() > 128);

        let document = [0x61; 16];
        assert_ne!(runtime.producer_identity().0, document);
        let mut base_build = M11CandidateBuild::new_with_persistent_source_facts(
            &mut runtime,
            document,
            [0x62; 16],
            source,
            1,
            1,
            scan_profile,
            M11RoleRecords::persistent(
                Box::<[u8]>::from(&b"base-green"[..]),
                Box::<[u8]>::from(&b"base-structural"[..]),
            )
            .expect("base role records"),
        )
        .expect("base candidate");
        base_build
            .offer_reference(
                &runtime,
                M11ReferenceRecord::new(
                    M11ReferenceRange::new(0..5, 0..5),
                    M11ReferenceRange::new(0..1, 0..1),
                    M11ReferenceRange::new(2..3, 2..3),
                    None,
                    Box::<[u8]>::from(&b"x"[..]),
                    Box::<[u8]>::from(&b"x"[..]),
                    None,
                ),
            )
            .expect("base reference");
        while !base_build.references_idle() {
            assert!(matches!(
                base_build
                    .poll(&mut runtime, 64)
                    .expect("drain base reference"),
                M11CandidateBuildPoll::Pending { .. }
            ));
        }
        base_build
            .finish_references(&runtime)
            .expect("finish base references");
        while matches!(
            base_build
                .poll(&mut runtime, 256)
                .expect("publish base candidate"),
            M11CandidateBuildPoll::Pending { .. }
        ) {}

        let base_publication = base_build.into_publication().expect("base publication");
        let mut base_stream = Box::new(base_publication)
            .into_snapshot_stream(&runtime)
            .expect("base snapshot stream");
        assert_eq!(
            base_stream.begin_frame().expect("base Begin").kind,
            M11SnapshotFrameKind::Begin
        );
        loop {
            match base_stream.poll(&runtime, 256).expect("traverse base") {
                M11OwnedSnapshotPoll::Pending { .. } | M11OwnedSnapshotPoll::Frame { .. } => {}
                M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                    panic!("full base traversal requested exact-base replay")
                }
            }
            if base_stream.traversal_complete {
                break;
            }
        }
        let mut base = base_stream
            .into_retained_publication(&runtime)
            .expect("retain traversed base");

        let base_reference_root = {
            let publication = base.publication().expect("retained base publication");
            let descriptor = decode_manifest_descriptor(
                runtime.producer_arena(),
                publication.root_id(),
                publication.authority(),
            )
            .expect("decode base manifest");
            let wrapper = descriptor.children[role_index(CandidateRole::References)];
            assert_eq!(
                runtime
                    .producer_arena()
                    .child_count(wrapper)
                    .expect("base References wrapper fanout"),
                1
            );
            runtime
                .producer_arena()
                .child_at(wrapper, 0)
                .expect("base canonical References root")
        };

        let mut target_build = M11CandidateBuild::
            new_with_persistent_source_facts_and_inline_projection_reusing_references(
                &mut runtime,
                document,
                [0x63; 16],
                source,
                2,
                1,
                scan_profile,
                M11RoleRecords::persistent(
                    Box::<[u8]>::from(&b"target-green"[..]),
                    Box::<[u8]>::from(&b"target-structural"[..]),
                )
                .expect("target role records"),
                &inline_root,
                &base,
            )
            .expect("exact-crop target candidate");

        inline_root
            .begin_release(&mut runtime)
            .expect("release original inline root");
        while !inline_root
            .poll_release(&mut runtime, 64)
            .expect("poll original inline release")
            .complete()
        {}

        while matches!(
            target_build
                .poll(&mut runtime, 256)
                .expect("publish exact-crop target"),
            M11CandidateBuildPoll::Pending { .. }
        ) {}
        let mut target = target_build
            .into_publication()
            .expect("exact-crop target publication");

        base.begin_close(&mut runtime).expect("close retained base");
        while !base
            .poll_close(&mut runtime, 64)
            .expect("poll retained base close")
        {}

        let publication = target.publication().expect("target publication");
        let descriptor = decode_manifest_descriptor(
            runtime.producer_arena(),
            publication.root_id(),
            publication.authority(),
        )
        .expect("decode target manifest after base release");
        let reference_wrapper = descriptor.children[role_index(CandidateRole::References)];
        let target_reference_root = runtime
            .producer_arena()
            .child_at(reference_wrapper, 0)
            .expect("target canonical References root");
        assert_eq!(target_reference_root, base_reference_root);
        runtime
            .producer_arena()
            .payload(target_reference_root)
            .expect("reused References root survives base release");

        let projection = descriptor.children[role_index(CandidateRole::Projection)];
        assert_eq!(
            descriptor.metadata[role_index(CandidateRole::Projection)].record_count,
            LOGICAL_PAGES as u64 + 2
        );
        assert_eq!(
            runtime
                .producer_arena()
                .child_count(projection)
                .expect("Projection wrapper fanout"),
            3
        );
        assert!(
            runtime
                .producer_arena()
                .child_count(projection)
                .expect("bounded Projection wrapper")
                <= M11_MAX_ROLE_RECORDS
        );

        target.begin_close(&mut runtime).expect("close target");
        while !target
            .poll_close(&mut runtime, 64)
            .expect("poll target close")
        {}
        runtime.begin_close().expect("close runtime");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    #[test]
    fn exact_block_target_reuses_references_and_preserves_late_block_queries() {
        let mut runtime = runtime_with_source("alpha\nbeta");
        let source = runtime.current_source_version().expect("current source");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        install_source_facts(&mut runtime, scan_profile, parser_profile);
        let document = [0x81; 16];
        let mut base =
            retained_reference_base(&mut runtime, source, scan_profile, document, [0x82; 16]);

        let base_reference_root = {
            let publication = base.publication().expect("base publication");
            let descriptor = decode_manifest(
                runtime.producer_arena(),
                publication.root_id(),
                publication.authority(),
            )
            .expect("base manifest");
            runtime
                .producer_arena()
                .child_at(
                    descriptor.children[role_index(CandidateRole::References)],
                    0,
                )
                .expect("base canonical References root")
        };

        let record = |bytes: &[u8]| M11BlockRoleRecord::new(bytes).expect("block record");
        let mut blocks = build_block_root(
            &mut runtime,
            [
                M11BlockSequenceEntry::paragraph(5, 5, 1, record(b"g-alpha"), record(b"p-alpha"))
                    .expect("first paragraph"),
                M11BlockSequenceEntry::blank(1, 1).expect("line break"),
                M11BlockSequenceEntry::paragraph(4, 4, 0, record(b"g-beta"), record(b"p-beta"))
                    .expect("second paragraph"),
            ],
        );
        let canonical_block_root = blocks.tree_root_id_for_test().expect("block tree");
        let mut target_build =
            M11CandidateBuild::new_with_persistent_source_facts_and_blocks_reusing_references(
                &mut runtime,
                document,
                [0x83; 16],
                source,
                2,
                1,
                scan_profile,
                &blocks,
                &base,
            )
            .expect("exact persistent-block target");
        release_block_root(&mut runtime, &mut blocks);
        while matches!(
            target_build
                .poll(&mut runtime, 256)
                .expect("publish exact block target"),
            M11CandidateBuildPoll::Pending { .. }
        ) {}
        let target_publication = target_build.into_publication().expect("target publication");

        close_retained(&mut runtime, &mut base);

        let publication = target_publication.publication().expect("target manifest");
        let descriptor = decode_manifest(
            runtime.producer_arena(),
            publication.root_id(),
            publication.authority(),
        )
        .expect("validated target manifest");
        assert_eq!(
            descriptor.metadata[role_index(CandidateRole::Green)].schema,
            PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA
        );
        assert_eq!(
            descriptor.metadata[role_index(CandidateRole::Projection)].schema,
            PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA
        );
        let roles = persistent_block_manifest_roles(
            runtime.producer_arena(),
            &descriptor,
            publication.authority(),
        )
        .expect("paired persistent-block roles");
        assert_eq!(roles.root, Some(canonical_block_root));
        assert_eq!(roles.claim.entry_count(), 3);
        let target_reference_root = runtime
            .producer_arena()
            .child_at(
                descriptor.children[role_index(CandidateRole::References)],
                0,
            )
            .expect("target canonical References root");
        assert_eq!(target_reference_root, base_reference_root);
        runtime
            .producer_arena()
            .payload(target_reference_root)
            .expect("reused References survive base close");

        let mut target = retain_publication(&runtime, target_publication);
        let located = target
            .locate_block_point(
                &runtime,
                M11BlockSequencePoint::new(6, 6, SourceBoundaryAffinity::After),
            )
            .expect("late block query")
            .expect("second paragraph");
        assert_eq!(located.byte_range(), 6..10);
        assert_eq!(located.utf16_range(), 6..10);
        assert_eq!(
            located.entry().green().expect("Green").as_bytes(),
            b"g-beta"
        );
        assert_eq!(
            located.entry().projection().expect("Projection").as_bytes(),
            b"p-beta"
        );

        close_retained(&mut runtime, &mut target);
        runtime.begin_close().expect("close runtime");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        let closed = runtime.arena_metrics();
        assert_eq!(closed.resident_nodes, 0);
        assert_eq!(closed.live_payload_bytes, 0);
        assert_eq!(closed.live_builds, 0);
        assert_eq!(closed.pending_build_aborts, 0);
        assert_eq!(closed.pending_reclaims, 0);
    }

    #[test]
    fn exact_recursive_green_target_requires_its_base_and_installs_independently() {
        let mut runtime = runtime_with_source("alpha\nbeta");
        let source = runtime.current_source_version().expect("current source");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        install_source_facts(&mut runtime, scan_profile, parser_profile);
        let document = [0xa1; 16];
        let mut base =
            retained_reference_base(&mut runtime, source, scan_profile, document, [0xa2; 16]);
        let base_reference_root = {
            let publication = base.publication().expect("base publication");
            let descriptor = decode_manifest(
                runtime.producer_arena(),
                publication.root_id(),
                publication.authority(),
            )
            .expect("base manifest");
            runtime
                .producer_arena()
                .child_at(
                    descriptor.children[role_index(CandidateRole::References)],
                    0,
                )
                .expect("base canonical References root")
        };
        let mut green = build_recursive_green_root(&mut runtime);

        let mismatch = match M11CandidateBuild::
            new_with_persistent_source_facts_and_recursive_green_reusing_references(
                &mut runtime,
                [0xaf; 16],
                [0xa3; 16],
                source,
                2,
                1,
                scan_profile,
                M11RoleRecords::persistent_recursive_green_projection_records([
                    Box::<[u8]>::from(&b"projection"[..]),
                ])
                .expect("Projection records"),
                &green,
                &base,
            )
        {
            Ok(_) => panic!("foreign base document was accepted"),
            Err(error) => error,
        };
        assert!(mismatch.is_cross_authority());

        let mut target_build = M11CandidateBuild::
            new_with_persistent_source_facts_and_recursive_green_reusing_references(
                &mut runtime,
                document,
                [0xa3; 16],
                source,
                2,
                1,
                scan_profile,
                M11RoleRecords::persistent_recursive_green_projection_records([
                    Box::<[u8]>::from(&b"projection"[..]),
                ])
                .expect("Projection records"),
                &green,
                &base,
            )
            .expect("exact recursive Green candidate");
        green
            .begin_release(&mut runtime)
            .expect("release original Green root");
        while !green
            .poll_release(&mut runtime, 64)
            .expect("poll original Green release")
            .complete()
        {}
        while matches!(
            target_build
                .poll(&mut runtime, 256)
                .expect("publish exact recursive Green target"),
            M11CandidateBuildPoll::Pending { .. }
        ) {}
        let target = target_build.into_publication().expect("target publication");
        close_retained(&mut runtime, &mut base);

        let published = target.publication().expect("target manifest");
        let descriptor = decode_manifest(
            runtime.producer_arena(),
            published.root_id(),
            published.authority(),
        )
        .expect("validated exact target manifest");
        assert_eq!(
            descriptor.metadata[role_index(CandidateRole::Green)].schema,
            PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA
        );
        let green_role = persistent_recursive_green_manifest_role(
            runtime.producer_arena(),
            &descriptor,
            published.authority(),
        )
        .expect("validated recursive Green role");
        assert_eq!(green_role.claim.event_count(), 3);
        assert_eq!(
            runtime
                .producer_arena()
                .child_at(
                    descriptor.children[role_index(CandidateRole::References)],
                    0,
                )
                .expect("target canonical References root"),
            base_reference_root
        );

        let mut stream = Box::new(target)
            .into_snapshot_stream(&runtime)
            .expect("target snapshot stream");
        let mut host = M11CandidateHost::new(document, source, 1).expect("independent host");
        let begin = stream.begin_frame().expect("snapshot Begin");
        host.begin_snapshot(&begin.bytes).expect("host Begin");
        loop {
            match stream.poll(&runtime, 256).expect("snapshot poll") {
                M11OwnedSnapshotPoll::Pending { .. } => {}
                M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                    panic!("full target snapshot requested exact-base replay")
                }
                M11OwnedSnapshotPoll::Frame { frame, .. } => match frame.kind {
                    M11SnapshotFrameKind::Node => host.offer_node(&frame.bytes).expect("host Node"),
                    M11SnapshotFrameKind::End => {
                        host.finish_snapshot(&frame.bytes).expect("host End");
                        break;
                    }
                    _ => panic!("full target snapshot emitted a delta frame"),
                },
            }
        }
        let installed = loop {
            let poll = host.poll_install(256).expect("host install");
            if let Some(installed) = poll.installed {
                break installed;
            }
        };
        let location = host
            .locate_recursive_green_point(
                installed,
                M11RecursiveGreenPoint::new(7, 7, SourceBoundaryAffinity::After),
            )
            .expect("host recursive Green query")
            .expect("Green content location");
        assert_eq!(location.owner().kind().get(), 1);
        assert_eq!(location.part(), M11RecursiveGreenCoveragePart::Content);

        stream
            .begin_close(&mut runtime)
            .expect("close target stream");
        while !stream
            .poll_close(&mut runtime, 256)
            .expect("poll target stream close")
        {}
        host.begin_close().expect("close host");
        while !host.poll_close(256).expect("poll host close") {}
        runtime.begin_close().expect("close runtime");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    #[test]
    fn cold_recursive_green_candidate_retains_the_session_reference_journal() {
        let mut runtime = runtime_with_source("alpha\nbeta");
        let source = runtime.current_source_version().expect("current source");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        install_source_facts(&mut runtime, scan_profile, parser_profile);
        let mut green = build_recursive_green_root(&mut runtime);
        let mut references = build_reference_journal_root(&mut runtime, source);

        let mut build =
            M11CandidateBuild::new_with_persistent_source_facts_recursive_green_and_reference_journal(
                &mut runtime,
                [0xb1; 16],
                [0xb2; 16],
                source,
                1,
                1,
                scan_profile,
                M11RoleRecords::persistent_recursive_green_projection_records([
                    Box::<[u8]>::from(&b"projection"[..]),
                ])
                .expect("Projection records"),
                &green,
                &references,
            )
            .expect("cold recursive Green candidate");
        assert!(
            !build.references_idle(),
            "retained References must not open a second reference builder"
        );

        green
            .begin_release(&mut runtime)
            .expect("release original Green root");
        while !green
            .poll_release(&mut runtime, 64)
            .expect("poll original Green release")
            .complete()
        {}
        references
            .begin_release(&mut runtime)
            .expect("release original References root");
        while !references
            .poll_release(&mut runtime, 64)
            .expect("poll original References release")
            .complete()
        {}

        while matches!(
            build
                .poll(&mut runtime, 256)
                .expect("publish cold recursive Green candidate"),
            M11CandidateBuildPoll::Pending { .. }
        ) {}
        let publication = build.into_publication().expect("candidate publication");
        let published = publication.publication().expect("published manifest");
        let descriptor = decode_manifest(
            runtime.producer_arena(),
            published.root_id(),
            published.authority(),
        )
        .expect("validated candidate manifest");
        assert_eq!(
            descriptor.metadata[role_index(CandidateRole::References)].record_count,
            1
        );
        assert_eq!(
            descriptor.metadata[role_index(CandidateRole::Green)].schema,
            PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA
        );
        let canonical_references = runtime
            .producer_arena()
            .child_at(
                descriptor.children[role_index(CandidateRole::References)],
                0,
            )
            .expect("canonical References root");
        runtime
            .producer_arena()
            .payload(canonical_references)
            .expect("candidate retains References after session-root release");

        let mut retained = retain_publication(&runtime, publication);
        close_retained(&mut runtime, &mut retained);
        runtime.begin_close().expect("close runtime");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    #[test]
    fn exact_block_target_rejects_source_base_and_runtime_binding_mismatches_without_leaks() {
        let mut runtime = runtime_with_source("alpha\nbeta");
        let source = runtime.current_source_version().expect("current source");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        install_source_facts(&mut runtime, scan_profile, parser_profile);
        let document = [0x91; 16];
        let mut base =
            retained_reference_base(&mut runtime, source, scan_profile, document, [0x92; 16]);
        let record = |bytes: &[u8]| M11BlockRoleRecord::new(bytes).expect("block record");
        let mut blocks = build_block_root(
            &mut runtime,
            [
                M11BlockSequenceEntry::paragraph(5, 5, 1, record(b"g0"), record(b"p0"))
                    .expect("first paragraph"),
                M11BlockSequenceEntry::blank(1, 1).expect("line break"),
                M11BlockSequenceEntry::paragraph(4, 4, 0, record(b"g1"), record(b"p1"))
                    .expect("second paragraph"),
            ],
        );

        let mut foreign = runtime_with_source("foreign");
        let foreign_source = foreign
            .current_source_version()
            .expect("foreign source authority");
        let mut foreign_blocks = build_block_root(
            &mut foreign,
            [M11BlockSequenceEntry::paragraph(
                7,
                7,
                0,
                record(b"foreign-green"),
                record(b"foreign-projection"),
            )
            .expect("foreign paragraph")],
        );

        drain_arena_reclaim(&mut runtime);
        let before = runtime.arena_metrics();
        let source_error =
            match M11CandidateBuild::new_with_persistent_source_facts_and_blocks_reusing_references(
                &mut runtime,
                document,
                [0x93; 16],
                foreign_source,
                2,
                1,
                scan_profile,
                &blocks,
                &base,
            ) {
                Ok(_) => panic!("foreign source authority was accepted"),
                Err(error) => error,
            };
        assert!(matches!(
            source_error.0,
            ErrorInner::Manifest(ManifestError::CrossAuthority)
        ));
        drain_arena_reclaim(&mut runtime);
        let after_source = runtime.arena_metrics();
        assert_eq!(after_source.resident_nodes, before.resident_nodes);
        assert_eq!(after_source.live_payload_bytes, before.live_payload_bytes);
        assert_eq!(after_source.live_builds, before.live_builds);
        assert_eq!(after_source.pending_build_aborts, 0);

        let block_error =
            match M11CandidateBuild::new_with_persistent_source_facts_and_blocks_reusing_references(
                &mut runtime,
                document,
                [0x94; 16],
                source,
                2,
                1,
                scan_profile,
                &foreign_blocks,
                &base,
            ) {
                Ok(_) => panic!("foreign block runtime binding was accepted"),
                Err(error) => error,
            };
        assert!(matches!(
            block_error.0,
            ErrorInner::Manifest(ManifestError::BlockSequence(
                M11BlockSequenceError::SourceAuthorityMismatch
            ))
        ));
        drain_arena_reclaim(&mut runtime);
        let after_blocks = runtime.arena_metrics();
        assert_eq!(after_blocks.resident_nodes, before.resident_nodes);
        assert_eq!(after_blocks.live_payload_bytes, before.live_payload_bytes);
        assert_eq!(after_blocks.live_builds, before.live_builds);
        assert_eq!(after_blocks.pending_build_aborts, 0);

        let base_error =
            match M11CandidateBuild::new_with_persistent_source_facts_and_blocks_reusing_references(
                &mut runtime,
                [0x95; 16],
                [0x96; 16],
                source,
                2,
                1,
                scan_profile,
                &blocks,
                &base,
            ) {
                Ok(_) => panic!("cross-document base was accepted"),
                Err(error) => error,
            };
        assert!(matches!(
            base_error.0,
            ErrorInner::Manifest(ManifestError::CrossAuthority)
        ));
        let after_base = runtime.arena_metrics();
        assert_eq!(after_base.resident_nodes, before.resident_nodes);
        assert_eq!(after_base.live_payload_bytes, before.live_payload_bytes);
        assert_eq!(after_base.live_builds, before.live_builds);

        release_block_root(&mut runtime, &mut blocks);
        close_retained(&mut runtime, &mut base);
        runtime.begin_close().expect("close runtime");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);

        release_block_root(&mut foreign, &mut foreign_blocks);
        foreign.begin_close().expect("close foreign runtime");
        while !foreign
            .poll_close(256)
            .expect("poll foreign runtime close")
            .complete
        {}
        assert_eq!(foreign.arena_metrics().resident_nodes, 0);
    }
}
