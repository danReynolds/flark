//! Persistent reference-definition index prototypes.
//!
//! The production-shaped initial builder streams one CandidateWriter-
//! authenticated occurrence at a time into global and per-label persistent
//! orderings. It owns cooked destination/title bytes, captures checkpoints in
//! constant retained state, and never groups a document in memory.
//!
//! This file also retains an older restart/re-winner topology proof. That
//! fixture proves fixed-rank suffix surgery, but it is not the production
//! restart endpoint: committed interner adoption/lookup and a persistent
//! replacement spool still need to replace its raw-ID and changed-interval
//! `Vec` seams. Those gaps remain explicit HOLDs.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Range;

use flark_reference_label_service::{normalize_reference_label, reference_label_length_is_valid};

use crate::arena::{
    ArenaBuildError, ArenaBuildOwner, ArenaBuildSession, ArenaBuildTicket, ArenaError, ArenaId,
    ArenaScopedId, OwnedArenaRef, PageArena,
};
use crate::persistent_blob::{
    PersistentBlobError, PersistentBlobSpec, PersistentByteBlob, PersistentByteBlobMetadata,
};
use crate::persistent_sequence::{
    ResumableSequenceProgress, ResumableSequenceSplice, ResumableSequenceSplitProgress,
    ResumableStreamingSequenceBuilder, SequenceMutationReceipt, SequenceNodeKind, SequenceSpec,
    sequence_node,
};
use crate::reference_label_interner::{
    CommittedReferenceLabelInterner, CommittedReferenceLabelLookup,
    CommittedReferenceLabelLookupProgress, InternedReferenceLabel, ReferenceLabelInternerAdoption,
    ReferenceLabelInternerError, ReferenceLabelInternerManifest,
    ReferenceLabelInternerManifestIdentity, ReferenceLabelUseAck,
};
use crate::{SourceRevision, SourceRootId, SourceSnapshotDescriptor};

const FORMAT_VERSION: u8 = 1;
const GLOBAL_OCCURRENCE_TAG: u8 = 0xb1;
const GLOBAL_BRANCH_TAG: u8 = 0xb2;
const LABEL_OCCURRENCE_TAG: u8 = 0xb3;
const LABEL_BRANCH_TAG: u8 = 0xb4;
const DIRECTORY_LEAF_TAG: u8 = 0xb5;
const DIRECTORY_BRANCH_TAG: u8 = 0xb6;
const CHECKPOINT_TAG: u8 = 0xb7;
const AUTHORITY_TAG: u8 = 0xb8;
const GREEN_TAG: u8 = 0xb9;
const DOCUMENT_TAG: u8 = 0xba;
const SUFFIX_ADOPTION_TAG: u8 = 0xbb;
const DONOR_RANGE_TAG: u8 = 0xbc;

const GLOBAL_OCCURRENCE_BYTES: usize = 80;
const OCCURRENCE_BRANCH_BYTES: usize = 40;
const LABEL_BRANCH_BYTES: usize = 48;
const LABEL_OCCURRENCE_BYTES: usize = 32;
const DIRECTORY_LEAF_BYTES: usize = 32;
const DIRECTORY_BRANCH_BYTES: usize = 40;
const CHECKPOINT_BYTES: usize = 64;
const AUTHORITY_BYTES: usize = 32;
const GREEN_BYTES: usize = 32;
const DOCUMENT_BYTES: usize = 104;
const SUFFIX_ADOPTION_BYTES: usize = 80;
const DONOR_RANGE_BYTES: usize = 160;

/// A corrupt or adversarial persisted sequence must not turn one planning
/// poll into unbounded synchronous work.  A 64-level AVL envelope admits far
/// more leaves than the arena can store while still giving the UI scheduler a
/// small, explicit admission bound.
const MAX_AUTHENTICATED_SEQUENCE_HEIGHT: u16 = 64;
const MAX_QUERY_SEQUENCE_NODES_PER_TASK: u64 = 512;

const DOCUMENT_BASE_CHILDREN: usize = 7;
const DOCUMENT_GLOBAL: usize = 0;
const DOCUMENT_DIRECTORY: usize = 1;
const DOCUMENT_CHECKPOINT: usize = 2;
const DOCUMENT_INTERNER: usize = 3;
const DOCUMENT_SOURCE_LINEAGE: usize = 4;
const DOCUMENT_GREEN: usize = 5;
const DOCUMENT_ALLOCATOR: usize = 6;
const DOCUMENT_ADOPTION: usize = 7;

const CHECKPOINT_PREFIX_DIRECTORY: usize = 0;
const CHECKPOINT_GREEN: usize = 1;

/// Historical scalar fixture used only by the restart-update regression
/// suite. Production construction publishes cooked values and never exposes
/// Crop pieces or projection-program numbers as durable authority.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FixtureRetainedOccurrenceCoordinate {
    source_piece: u64,
    source_piece_offset: u64,
    source_length: u64,
    projection_program: u64,
    projection_logical_offset: u64,
}

#[cfg(test)]
impl FixtureRetainedOccurrenceCoordinate {
    fn validate(self) -> Result<(), RestartIndexError> {
        if self.source_piece == 0
            || self.source_length == 0
            || self.projection_program == 0
            || self
                .source_piece_offset
                .checked_add(self.source_length)
                .is_none()
        {
            return Err(RestartIndexError::Invalid(
                "occurrence coordinate is not a retained source/projection capability",
            ));
        }
        Ok(())
    }
}

/// Published reference values are independent of finite source-history
/// retention. The writer cooks destination/title bytes transactionally, and
/// this descriptor owns those immutable blob roots. Exact parser ranges and
/// projection transforms are admission evidence, not long-lived semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CookedReferenceValueDescriptor {
    destination_root: Option<ArenaId>,
    destination_bytes: u64,
    title: Option<CookedReferenceBlobDescriptor>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CookedReferenceBlobDescriptor {
    root: Option<ArenaId>,
    bytes: u64,
}

/// The legacy variant remains test-only so the established restart/re-winner
/// corpus can continue checking topology while the concrete cooked-value
/// adapter is wired. It cannot enter a production build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReferenceOccurrenceValue {
    Cooked(CookedReferenceValueDescriptor),
    #[cfg(test)]
    FixtureLegacy {
        destination_id: u64,
        coordinate_generation: u64,
        coordinate: FixtureRetainedOccurrenceCoordinate,
    },
}

/// Fixed-size semantic descriptor. Source/projection cuts do not survive
/// publication; only the cooked value roots do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReferenceOccurrence {
    occurrence_id: u64,
    label_id: u64,
    value: ReferenceOccurrenceValue,
}

impl ReferenceOccurrence {
    fn validate(self) -> Result<(), RestartIndexError> {
        if self.occurrence_id == 0 || self.label_id == 0 {
            return Err(RestartIndexError::Invalid(
                "reference occurrence has a zero exact identity",
            ));
        }
        match self.value {
            ReferenceOccurrenceValue::Cooked(value) => {
                if value.destination_root.is_none() && value.destination_bytes != 0
                    || value
                        .title
                        .is_some_and(|title| title.root.is_none() && title.bytes != 0)
                {
                    return Err(RestartIndexError::Invalid(
                        "cooked reference value descriptor is incomplete",
                    ));
                }
                Ok(())
            }
            #[cfg(test)]
            ReferenceOccurrenceValue::FixtureLegacy {
                destination_id,
                coordinate_generation: _,
                coordinate,
            } => {
                if destination_id == 0 {
                    return Err(RestartIndexError::Invalid(
                        "fixture occurrence has a zero scalar identity",
                    ));
                }
                coordinate.validate()
            }
        }
    }

    #[cfg(test)]
    const fn fixture_coordinate_generation(self) -> Option<u64> {
        match self.value {
            ReferenceOccurrenceValue::Cooked(_) => None,
            ReferenceOccurrenceValue::FixtureLegacy {
                coordinate_generation,
                ..
            } => Some(coordinate_generation),
        }
    }

    const fn crosses_writer_source_revision(self, expected: u64) -> bool {
        match self.value {
            ReferenceOccurrenceValue::Cooked(_) => false,
            #[cfg(test)]
            ReferenceOccurrenceValue::FixtureLegacy {
                coordinate_generation,
                ..
            } => coordinate_generation != expected,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OccurrenceSummary {
    leaves: u64,
    height: u16,
    first_occurrence: u64,
    last_occurrence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LabelOccurrenceSummary {
    leaves: u64,
    height: u16,
    first_occurrence: u64,
    last_occurrence: u64,
    label_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectorySummary {
    leaves: u64,
    height: u16,
    minimum_label: u64,
    maximum_label: u64,
}

#[derive(Debug)]
struct GlobalOccurrenceSpec;

#[derive(Debug)]
struct LabelOccurrenceSpec;

#[derive(Debug)]
struct ExactLabelDirectorySpec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestartIndexError {
    Arena(ArenaError),
    ArenaBuild(ArenaBuildError),
    PersistentBlob(PersistentBlobError),
    ReferenceLabelInterner(ReferenceLabelInternerError),
    Invalid(&'static str),
    InjectedFault(u64),
}

impl From<ArenaError> for RestartIndexError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<ArenaBuildError> for RestartIndexError {
    fn from(value: ArenaBuildError) -> Self {
        Self::ArenaBuild(value)
    }
}

impl From<PersistentBlobError> for RestartIndexError {
    fn from(value: PersistentBlobError) -> Self {
        Self::PersistentBlob(value)
    }
}

impl fmt::Display for RestartIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::ArenaBuild(error) => error.fmt(formatter),
            Self::PersistentBlob(error) => error.fmt(formatter),
            Self::ReferenceLabelInterner(error) => error.fmt(formatter),
            Self::Invalid(message) => formatter.write_str(message),
            Self::InjectedFault(step) => {
                write!(formatter, "injected restart-index fault at task {step}")
            }
        }
    }
}

impl std::error::Error for RestartIndexError {}

impl From<ReferenceLabelInternerError> for RestartIndexError {
    fn from(value: ReferenceLabelInternerError) -> Self {
        Self::ReferenceLabelInterner(value)
    }
}

impl SequenceSpec for GlobalOccurrenceSpec {
    type Summary = OccurrenceSummary;
    type Error = RestartIndexError;
    type BranchPayload = [u8; OCCURRENCE_BRANCH_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(GLOBAL_OCCURRENCE_TAG) {
            return Ok(None);
        }
        let occurrence = decode_global_occurrence_payload(payload)?;
        Ok(Some(OccurrenceSummary {
            leaves: 1,
            height: 1,
            first_occurrence: occurrence.occurrence_id,
            last_occurrence: occurrence.occurrence_id,
        }))
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        decode_occurrence_branch(payload, GLOBAL_BRANCH_TAG)
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        encode_occurrence_branch(GLOBAL_BRANCH_TAG, summary)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        combine_occurrence_summary(left, right)
    }

    fn leaves(summary: Self::Summary) -> u64 {
        summary.leaves
    }

    fn height(summary: Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        RestartIndexError::Invalid(message)
    }
}

impl SequenceSpec for LabelOccurrenceSpec {
    type Summary = LabelOccurrenceSummary;
    type Error = RestartIndexError;
    type BranchPayload = [u8; LABEL_BRANCH_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(LABEL_OCCURRENCE_TAG) {
            return Ok(None);
        }
        let leaf = decode_label_occurrence(payload)?;
        Ok(Some(LabelOccurrenceSummary {
            leaves: 1,
            height: 1,
            first_occurrence: leaf.occurrence_id,
            last_occurrence: leaf.occurrence_id,
            label_id: leaf.label_id,
        }))
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(LABEL_BRANCH_TAG) {
            return Ok(None);
        }
        if payload.len() != LABEL_BRANCH_BYTES || payload[1] != FORMAT_VERSION {
            return Err(RestartIndexError::Invalid("invalid per-label branch"));
        }
        let summary = LabelOccurrenceSummary {
            leaves: get_u64(payload, 8),
            height: get_u16(payload, 16),
            first_occurrence: get_u64(payload, 24),
            last_occurrence: get_u64(payload, 32),
            label_id: get_u64(payload, 40),
        };
        if summary.leaves < 2
            || summary.height < 2
            || summary.first_occurrence == 0
            || summary.last_occurrence == 0
            || summary.label_id == 0
        {
            return Err(RestartIndexError::Invalid(
                "corrupt per-label branch summary",
            ));
        }
        Ok(Some(summary))
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        let mut payload = [0_u8; LABEL_BRANCH_BYTES];
        payload[0] = LABEL_BRANCH_TAG;
        payload[1] = FORMAT_VERSION;
        put_u64(&mut payload, 8, summary.leaves);
        put_u16(&mut payload, 16, summary.height);
        put_u64(&mut payload, 24, summary.first_occurrence);
        put_u64(&mut payload, 32, summary.last_occurrence);
        put_u64(&mut payload, 40, summary.label_id);
        payload
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        if left.label_id != right.label_id {
            return Err(RestartIndexError::Invalid(
                "per-label sequence joined different exact labels",
            ));
        }
        Ok(LabelOccurrenceSummary {
            leaves: left
                .leaves
                .checked_add(right.leaves)
                .ok_or(RestartIndexError::Invalid("per-label length overflow"))?,
            height: left
                .height
                .max(right.height)
                .checked_add(1)
                .ok_or(RestartIndexError::Invalid("per-label height overflow"))?,
            first_occurrence: left.first_occurrence,
            last_occurrence: right.last_occurrence,
            label_id: left.label_id,
        })
    }

    fn leaves(summary: Self::Summary) -> u64 {
        summary.leaves
    }

    fn height(summary: Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        RestartIndexError::Invalid(message)
    }
}

impl SequenceSpec for ExactLabelDirectorySpec {
    type Summary = DirectorySummary;
    type Error = RestartIndexError;
    type BranchPayload = [u8; DIRECTORY_BRANCH_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(DIRECTORY_LEAF_TAG) {
            return Ok(None);
        }
        let leaf = decode_directory_leaf(payload)?;
        Ok(Some(DirectorySummary {
            leaves: 1,
            height: 1,
            minimum_label: leaf.label_id,
            maximum_label: leaf.label_id,
        }))
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(DIRECTORY_BRANCH_TAG) {
            return Ok(None);
        }
        if payload.len() != DIRECTORY_BRANCH_BYTES || payload[1] != FORMAT_VERSION {
            return Err(RestartIndexError::Invalid("invalid label-directory branch"));
        }
        let summary = DirectorySummary {
            leaves: get_u64(payload, 8),
            height: get_u16(payload, 16),
            minimum_label: get_u64(payload, 24),
            maximum_label: get_u64(payload, 32),
        };
        if summary.leaves < 2
            || summary.height < 2
            || summary.minimum_label == 0
            || summary.minimum_label >= summary.maximum_label
        {
            return Err(RestartIndexError::Invalid(
                "corrupt label-directory branch summary",
            ));
        }
        Ok(Some(summary))
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        let mut payload = [0_u8; DIRECTORY_BRANCH_BYTES];
        payload[0] = DIRECTORY_BRANCH_TAG;
        payload[1] = FORMAT_VERSION;
        put_u64(&mut payload, 8, summary.leaves);
        put_u16(&mut payload, 16, summary.height);
        put_u64(&mut payload, 24, summary.minimum_label);
        put_u64(&mut payload, 32, summary.maximum_label);
        payload
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        if left.maximum_label >= right.minimum_label {
            return Err(RestartIndexError::Invalid(
                "label-directory leaves are not exact and strictly ordered",
            ));
        }
        Ok(DirectorySummary {
            leaves: left
                .leaves
                .checked_add(right.leaves)
                .ok_or(RestartIndexError::Invalid(
                    "label-directory length overflow",
                ))?,
            height: left.height.max(right.height).checked_add(1).ok_or(
                RestartIndexError::Invalid("label-directory height overflow"),
            )?,
            minimum_label: left.minimum_label,
            maximum_label: right.maximum_label,
        })
    }

    fn leaves(summary: Self::Summary) -> u64 {
        summary.leaves
    }

    fn height(summary: Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        RestartIndexError::Invalid(message)
    }
}

fn combine_occurrence_summary(
    left: OccurrenceSummary,
    right: OccurrenceSummary,
) -> Result<OccurrenceSummary, RestartIndexError> {
    Ok(OccurrenceSummary {
        leaves: left
            .leaves
            .checked_add(right.leaves)
            .ok_or(RestartIndexError::Invalid(
                "occurrence sequence length overflow",
            ))?,
        height: left
            .height
            .max(right.height)
            .checked_add(1)
            .ok_or(RestartIndexError::Invalid(
                "occurrence sequence height overflow",
            ))?,
        first_occurrence: left.first_occurrence,
        last_occurrence: right.last_occurrence,
    })
}

fn encode_occurrence_branch(tag: u8, summary: OccurrenceSummary) -> [u8; OCCURRENCE_BRANCH_BYTES] {
    let mut payload = [0_u8; OCCURRENCE_BRANCH_BYTES];
    payload[0] = tag;
    payload[1] = FORMAT_VERSION;
    put_u64(&mut payload, 8, summary.leaves);
    put_u16(&mut payload, 16, summary.height);
    put_u64(&mut payload, 24, summary.first_occurrence);
    put_u64(&mut payload, 32, summary.last_occurrence);
    payload
}

fn decode_occurrence_branch(
    payload: &[u8],
    tag: u8,
) -> Result<Option<OccurrenceSummary>, RestartIndexError> {
    if payload.first().copied() != Some(tag) {
        return Ok(None);
    }
    if payload.len() != OCCURRENCE_BRANCH_BYTES || payload[1] != FORMAT_VERSION {
        return Err(RestartIndexError::Invalid("invalid occurrence branch"));
    }
    let summary = OccurrenceSummary {
        leaves: get_u64(payload, 8),
        height: get_u16(payload, 16),
        first_occurrence: get_u64(payload, 24),
        last_occurrence: get_u64(payload, 32),
    };
    if summary.leaves < 2
        || summary.height < 2
        || summary.first_occurrence == 0
        || summary.last_occurrence == 0
    {
        return Err(RestartIndexError::Invalid(
            "corrupt occurrence branch summary",
        ));
    }
    Ok(Some(summary))
}

fn encode_global_occurrence(occurrence: ReferenceOccurrence) -> [u8; GLOBAL_OCCURRENCE_BYTES] {
    let mut payload = [0_u8; GLOBAL_OCCURRENCE_BYTES];
    payload[0] = GLOBAL_OCCURRENCE_TAG;
    payload[1] = FORMAT_VERSION;
    put_u64(&mut payload, 8, occurrence.occurrence_id);
    put_u64(&mut payload, 16, occurrence.label_id);
    match occurrence.value {
        ReferenceOccurrenceValue::Cooked(value) => {
            payload[2] = 2;
            payload[3] = u8::from(value.destination_root.is_some())
                | (u8::from(value.title.is_some()) << 1)
                | (u8::from(value.title.is_some_and(|title| title.root.is_some())) << 2);
            put_u64(&mut payload, 24, value.destination_bytes);
            put_u64(&mut payload, 32, value.title.map_or(0, |title| title.bytes));
            if let Some(destination) = value.destination_root {
                put_arena_id(&mut payload, 40, destination);
            }
            if let Some(title) = value.title.and_then(|title| title.root) {
                put_arena_id(&mut payload, 48, title);
            }
        }
        #[cfg(test)]
        ReferenceOccurrenceValue::FixtureLegacy {
            destination_id,
            coordinate_generation,
            coordinate,
        } => {
            payload[2] = 1;
            put_u64(&mut payload, 24, destination_id);
            put_u64(&mut payload, 32, coordinate.source_piece);
            put_u64(&mut payload, 40, coordinate.source_piece_offset);
            put_u64(&mut payload, 48, coordinate.source_length);
            put_u64(&mut payload, 56, coordinate.projection_program);
            put_u64(&mut payload, 64, coordinate.projection_logical_offset);
            put_u64(&mut payload, 72, coordinate_generation);
        }
    }
    payload
}

fn decode_global_occurrence_payload(
    payload: &[u8],
) -> Result<ReferenceOccurrence, RestartIndexError> {
    if payload.len() != GLOBAL_OCCURRENCE_BYTES
        || payload[0] != GLOBAL_OCCURRENCE_TAG
        || payload[1] != FORMAT_VERSION
    {
        return Err(RestartIndexError::Invalid(
            "invalid reference occurrence leaf",
        ));
    }
    let value = match payload[2] {
        2 if payload[3] <= 0b111 && (payload[3] & 0b100 == 0 || payload[3] & 0b010 != 0) => {
            let flags = payload[3];
            ReferenceOccurrenceValue::Cooked(CookedReferenceValueDescriptor {
                destination_root: (flags & 0b001 != 0).then(|| get_arena_id(payload, 40)),
                destination_bytes: get_u64(payload, 24),
                title: (flags & 0b010 != 0).then(|| CookedReferenceBlobDescriptor {
                    root: (flags & 0b100 != 0).then(|| get_arena_id(payload, 48)),
                    bytes: get_u64(payload, 32),
                }),
            })
        }
        #[cfg(test)]
        1 => ReferenceOccurrenceValue::FixtureLegacy {
            destination_id: get_u64(payload, 24),
            coordinate_generation: get_u64(payload, 72),
            coordinate: FixtureRetainedOccurrenceCoordinate {
                source_piece: get_u64(payload, 32),
                source_piece_offset: get_u64(payload, 40),
                source_length: get_u64(payload, 48),
                projection_program: get_u64(payload, 56),
                projection_logical_offset: get_u64(payload, 64),
            },
        },
        _ => {
            return Err(RestartIndexError::Invalid(
                "unknown reference occurrence value storage mode",
            ));
        }
    };
    let occurrence = ReferenceOccurrence {
        occurrence_id: get_u64(payload, 8),
        label_id: get_u64(payload, 16),
        value,
    };
    occurrence.validate()?;
    Ok(occurrence)
}

fn decode_global_occurrence(
    arena: &PageArena,
    leaf: ArenaId,
) -> Result<ReferenceOccurrence, RestartIndexError> {
    let occurrence = decode_global_occurrence_payload(arena.payload(leaf)?)?;
    match occurrence.value {
        ReferenceOccurrenceValue::Cooked(value) => {
            validate_cooked_reference_blob(arena, value.destination_root, value.destination_bytes)?;
            if let Some(title) = value.title {
                validate_cooked_reference_blob(arena, title.root, title.bytes)?;
            }
            let roots = [
                value.destination_root,
                value.title.and_then(|title| title.root),
            ];
            let mut expected_index = 0;
            for root in roots.into_iter().flatten() {
                if arena.packed_child_at(leaf, expected_index)? != root {
                    return Err(RestartIndexError::Invalid(
                        "cooked reference occurrence crossed its owned value blobs",
                    ));
                }
                expected_index += 1;
            }
            if arena.packed_child_count(leaf)? != expected_index {
                return Err(RestartIndexError::Invalid(
                    "cooked reference occurrence crossed its owned value blobs",
                ));
            }
        }
        #[cfg(test)]
        ReferenceOccurrenceValue::FixtureLegacy { .. } => {
            if arena.packed_child_count(leaf)? != 0 {
                return Err(RestartIndexError::Invalid(
                    "fixture reference occurrence unexpectedly owns value blobs",
                ));
            }
        }
    }
    Ok(occurrence)
}

fn validate_cooked_reference_blob(
    arena: &PageArena,
    root: Option<ArenaId>,
    bytes: u64,
) -> Result<(), RestartIndexError> {
    match root {
        None if bytes == 0 => Ok(()),
        Some(root) => {
            let summary = sequence_node::<PersistentBlobSpec>(arena, root)?.0;
            if summary.bytes != bytes {
                return Err(RestartIndexError::Invalid(
                    "cooked reference blob length crossed its persistent root",
                ));
            }
            Ok(())
        }
        None => Err(RestartIndexError::Invalid(
            "nonempty cooked reference blob has no persistent root",
        )),
    }
}

fn allocate_global_occurrence_leaf(
    session: &mut ArenaBuildSession<'_>,
    occurrence: ReferenceOccurrence,
) -> Result<ArenaBuildOwner, RestartIndexError> {
    let payload = encode_global_occurrence(occurrence);
    let (owner, _) = match occurrence.value {
        ReferenceOccurrenceValue::Cooked(value) => {
            match (
                value.destination_root,
                value.title.and_then(|title| title.root),
            ) {
                (Some(destination), Some(title)) => {
                    session.allocate_packed(&payload, &[destination, title])?
                }
                (Some(destination), None) => session.allocate_packed(&payload, &[destination])?,
                (None, Some(title)) => session.allocate_packed(&payload, &[title])?,
                (None, None) => session.allocate_packed(&payload, &[])?,
            }
        }
        #[cfg(test)]
        ReferenceOccurrenceValue::FixtureLegacy { .. } => session.allocate_packed(&payload, &[])?,
    };
    Ok(owner)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LabelOccurrenceLeaf {
    occurrence_id: u64,
    label_id: u64,
}

fn encode_label_occurrence(occurrence: ReferenceOccurrence) -> [u8; LABEL_OCCURRENCE_BYTES] {
    let mut payload = [0_u8; LABEL_OCCURRENCE_BYTES];
    payload[0] = LABEL_OCCURRENCE_TAG;
    payload[1] = FORMAT_VERSION;
    put_u64(&mut payload, 8, occurrence.occurrence_id);
    put_u64(&mut payload, 16, occurrence.label_id);
    payload
}

fn decode_label_occurrence(payload: &[u8]) -> Result<LabelOccurrenceLeaf, RestartIndexError> {
    if payload.len() != LABEL_OCCURRENCE_BYTES
        || payload[0] != LABEL_OCCURRENCE_TAG
        || payload[1] != FORMAT_VERSION
    {
        return Err(RestartIndexError::Invalid(
            "invalid per-label occurrence leaf",
        ));
    }
    let leaf = LabelOccurrenceLeaf {
        occurrence_id: get_u64(payload, 8),
        label_id: get_u64(payload, 16),
    };
    if leaf.occurrence_id == 0 || leaf.label_id == 0 {
        return Err(RestartIndexError::Invalid(
            "per-label occurrence leaf has a zero identity",
        ));
    }
    Ok(leaf)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectoryLeaf {
    label_id: u64,
    sequence_length: u64,
}

fn encode_directory_leaf(label_id: u64, sequence_length: u64) -> [u8; DIRECTORY_LEAF_BYTES] {
    let mut payload = [0_u8; DIRECTORY_LEAF_BYTES];
    payload[0] = DIRECTORY_LEAF_TAG;
    payload[1] = FORMAT_VERSION;
    put_u64(&mut payload, 8, label_id);
    put_u64(&mut payload, 16, sequence_length);
    payload
}

fn decode_directory_leaf(payload: &[u8]) -> Result<DirectoryLeaf, RestartIndexError> {
    if payload.len() != DIRECTORY_LEAF_BYTES
        || payload[0] != DIRECTORY_LEAF_TAG
        || payload[1] != FORMAT_VERSION
    {
        return Err(RestartIndexError::Invalid("invalid label-directory leaf"));
    }
    let leaf = DirectoryLeaf {
        label_id: get_u64(payload, 8),
        sequence_length: get_u64(payload, 16),
    };
    if leaf.label_id == 0 {
        return Err(RestartIndexError::Invalid(
            "label-directory leaf has a zero exact label",
        ));
    }
    Ok(leaf)
}

fn put_u16(output: &mut [u8], offset: usize, value: u16) {
    output[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn get_u16(input: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(input[offset..offset + 2].try_into().expect("fixed u16"))
}

fn put_u64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn get_u64(input: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(input[offset..offset + 8].try_into().expect("fixed u64"))
}

fn put_arena_id(output: &mut [u8], offset: usize, value: ArenaId) {
    output[offset..offset + 4].copy_from_slice(&value.index.to_le_bytes());
    output[offset + 4..offset + 8].copy_from_slice(&value.generation.to_le_bytes());
}

fn get_arena_id(input: &[u8], offset: usize) -> ArenaId {
    ArenaId {
        index: u32::from_le_bytes(input[offset..offset + 4].try_into().expect("fixed u32")),
        generation: u32::from_le_bytes(
            input[offset + 4..offset + 8].try_into().expect("fixed u32"),
        ),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct QueryWorkReceipt {
    sequence_nodes_visited: u64,
}

impl QueryWorkReceipt {
    fn visit(&mut self) -> Result<(), RestartIndexError> {
        self.sequence_nodes_visited = self
            .sequence_nodes_visited
            .checked_add(1)
            .ok_or(RestartIndexError::Invalid("query work receipt overflow"))?;
        Ok(())
    }
}

fn counted_sequence_node<Spec: SequenceSpec<Error = RestartIndexError>>(
    arena: &PageArena,
    node: ArenaId,
    receipt: &mut QueryWorkReceipt,
) -> Result<(Spec::Summary, SequenceNodeKind), RestartIndexError> {
    receipt.visit()?;
    let decoded = sequence_node::<Spec>(arena, node)?;
    if Spec::height(decoded.0) > MAX_AUTHENTICATED_SEQUENCE_HEIGHT {
        return Err(RestartIndexError::Invalid(
            "persistent sequence exceeds the authenticated query-height envelope",
        ));
    }
    Ok(decoded)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectoryLookup {
    insertion_index: u64,
    leaf: Option<ArenaId>,
    sequence_root: Option<ArenaId>,
    sequence_length: u64,
}

fn sequence_length<Spec: SequenceSpec<Error = RestartIndexError>>(
    arena: &PageArena,
    root: Option<ArenaId>,
) -> Result<u64, RestartIndexError> {
    root.map(|root| sequence_node::<Spec>(arena, root).map(|value| Spec::leaves(value.0)))
        .transpose()
        .map(Option::unwrap_or_default)
}

fn locate_sequence_leaf<Spec: SequenceSpec<Error = RestartIndexError>>(
    arena: &PageArena,
    root: Option<ArenaId>,
    index: u64,
) -> Result<Option<ArenaId>, RestartIndexError> {
    locate_sequence_leaf_counted::<Spec>(arena, root, index, &mut QueryWorkReceipt::default())
}

fn locate_sequence_leaf_counted<Spec: SequenceSpec<Error = RestartIndexError>>(
    arena: &PageArena,
    root: Option<ArenaId>,
    index: u64,
    receipt: &mut QueryWorkReceipt,
) -> Result<Option<ArenaId>, RestartIndexError> {
    let Some(mut node) = root else {
        return Ok(None);
    };
    let total = Spec::leaves(counted_sequence_node::<Spec>(arena, node, receipt)?.0);
    if index >= total {
        return Ok(None);
    }
    let mut remaining = index;
    loop {
        match counted_sequence_node::<Spec>(arena, node, receipt)?.1 {
            SequenceNodeKind::Leaf => return Ok(Some(node)),
            SequenceNodeKind::Branch { left, right } => {
                let left_leaves =
                    Spec::leaves(counted_sequence_node::<Spec>(arena, left, receipt)?.0);
                if remaining < left_leaves {
                    node = left;
                } else {
                    remaining -= left_leaves;
                    node = right;
                }
            }
        }
    }
}

fn lookup_directory(
    arena: &PageArena,
    root: Option<ArenaId>,
    label_id: u64,
) -> Result<DirectoryLookup, RestartIndexError> {
    lookup_directory_counted(arena, root, label_id, &mut QueryWorkReceipt::default())
}

fn lookup_directory_counted(
    arena: &PageArena,
    root: Option<ArenaId>,
    label_id: u64,
    receipt: &mut QueryWorkReceipt,
) -> Result<DirectoryLookup, RestartIndexError> {
    if label_id == 0 {
        return Err(RestartIndexError::Invalid("zero exact label query"));
    }
    let Some(mut node) = root else {
        return Ok(DirectoryLookup {
            insertion_index: 0,
            leaf: None,
            sequence_root: None,
            sequence_length: 0,
        });
    };
    let mut prefix_leaves = 0_u64;
    loop {
        let (summary, kind) =
            counted_sequence_node::<ExactLabelDirectorySpec>(arena, node, receipt)?;
        match kind {
            SequenceNodeKind::Leaf => {
                let leaf = decode_directory_leaf(arena.payload(node)?)?;
                if leaf.label_id != label_id {
                    let insertion_index = prefix_leaves
                        .checked_add(u64::from(leaf.label_id < label_id))
                        .ok_or(RestartIndexError::Invalid(
                            "label-directory insertion rank overflow",
                        ))?;
                    return Ok(DirectoryLookup {
                        insertion_index,
                        leaf: None,
                        sequence_root: None,
                        sequence_length: 0,
                    });
                }
                let child_count = arena.packed_child_count(node)?;
                let sequence_root = match child_count {
                    0 => None,
                    1 => Some(arena.packed_child_at(node, 0)?),
                    _ => {
                        return Err(RestartIndexError::Invalid(
                            "label-directory leaf has multiple sequence roots",
                        ));
                    }
                };
                let actual = sequence_root
                    .map(|root| {
                        counted_sequence_node::<LabelOccurrenceSpec>(arena, root, receipt)
                            .map(|value| LabelOccurrenceSpec::leaves(value.0))
                    })
                    .transpose()?
                    .unwrap_or_default();
                let actual_label = sequence_root
                    .map(|root| {
                        counted_sequence_node::<LabelOccurrenceSpec>(arena, root, receipt)
                            .map(|value| value.0.label_id)
                    })
                    .transpose()?;
                if actual != leaf.sequence_length
                    || (leaf.sequence_length == 0) != sequence_root.is_none()
                    || actual_label.is_some_and(|label| label != leaf.label_id)
                {
                    return Err(RestartIndexError::Invalid(
                        "label-directory leaf length disagrees with its sequence",
                    ));
                }
                return Ok(DirectoryLookup {
                    insertion_index: prefix_leaves,
                    leaf: Some(node),
                    sequence_root,
                    sequence_length: leaf.sequence_length,
                });
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary =
                    counted_sequence_node::<ExactLabelDirectorySpec>(arena, left, receipt)?.0;
                if label_id <= left_summary.maximum_label {
                    node = left;
                } else {
                    prefix_leaves = prefix_leaves.checked_add(left_summary.leaves).ok_or(
                        RestartIndexError::Invalid("label-directory query rank overflow"),
                    )?;
                    node = right;
                }
                if label_id < summary.minimum_label {
                    // Descending left still finds the exact lower bound.
                    continue;
                }
            }
        }
    }
}

fn global_occurrence_at(
    arena: &PageArena,
    root: Option<ArenaId>,
    index: u64,
) -> Result<(ArenaId, ReferenceOccurrence), RestartIndexError> {
    let leaf = locate_sequence_leaf::<GlobalOccurrenceSpec>(arena, root, index)?.ok_or(
        RestartIndexError::Invalid("global occurrence index is out of bounds"),
    )?;
    Ok((leaf, decode_global_occurrence(arena, leaf)?))
}

fn global_occurrence_at_counted(
    arena: &PageArena,
    root: Option<ArenaId>,
    index: u64,
    receipt: &mut QueryWorkReceipt,
) -> Result<(ArenaId, ReferenceOccurrence), RestartIndexError> {
    let leaf = locate_sequence_leaf_counted::<GlobalOccurrenceSpec>(arena, root, index, receipt)?
        .ok_or(RestartIndexError::Invalid(
        "global occurrence index is out of bounds",
    ))?;
    Ok((leaf, decode_global_occurrence(arena, leaf)?))
}

fn per_label_occurrence_at(
    arena: &PageArena,
    root: Option<ArenaId>,
    index: u64,
) -> Result<(ArenaId, ArenaId, ReferenceOccurrence), RestartIndexError> {
    let leaf = locate_sequence_leaf::<LabelOccurrenceSpec>(arena, root, index)?.ok_or(
        RestartIndexError::Invalid("per-label occurrence index is out of bounds"),
    )?;
    let label_leaf = decode_label_occurrence(arena.payload(leaf)?)?;
    if arena.packed_child_count(leaf)? != 1 {
        return Err(RestartIndexError::Invalid(
            "per-label occurrence does not own one global descriptor",
        ));
    }
    let descriptor = arena.packed_child_at(leaf, 0)?;
    let occurrence = decode_global_occurrence(arena, descriptor)?;
    if occurrence.occurrence_id != label_leaf.occurrence_id
        || occurrence.label_id != label_leaf.label_id
    {
        return Err(RestartIndexError::Invalid(
            "per-label occurrence and global descriptor disagree",
        ));
    }
    Ok((leaf, descriptor, occurrence))
}

fn per_label_occurrence_at_counted(
    arena: &PageArena,
    root: Option<ArenaId>,
    index: u64,
    receipt: &mut QueryWorkReceipt,
) -> Result<(ArenaId, ArenaId, ReferenceOccurrence), RestartIndexError> {
    let leaf = locate_sequence_leaf_counted::<LabelOccurrenceSpec>(arena, root, index, receipt)?
        .ok_or(RestartIndexError::Invalid(
            "per-label occurrence index is out of bounds",
        ))?;
    let label_leaf = decode_label_occurrence(arena.payload(leaf)?)?;
    if arena.packed_child_count(leaf)? != 1 {
        return Err(RestartIndexError::Invalid(
            "per-label occurrence does not own one global descriptor",
        ));
    }
    let descriptor = arena.packed_child_at(leaf, 0)?;
    let occurrence = decode_global_occurrence(arena, descriptor)?;
    if occurrence.occurrence_id != label_leaf.occurrence_id
        || occurrence.label_id != label_leaf.label_id
    {
        return Err(RestartIndexError::Invalid(
            "per-label occurrence and global descriptor disagree",
        ));
    }
    Ok((leaf, descriptor, occurrence))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CheckpointManifest {
    source_revision: u64,
    occurrence_high_water: u64,
    interner_generation: u64,
    source_lineage_generation: u64,
    prefix_directory: Option<ArenaId>,
    green_root: ArenaId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DocumentManifest {
    source_revision: u64,
    source_snapshot: Option<SourceSnapshotDescriptor>,
    parent_source_revision: u64,
    occurrence_count: u64,
    restart_high_water: u64,
    old_join_high_water: u64,
    replacement_count: u64,
    interner_generation: u64,
    source_lineage_generation: u64,
    occurrence_allocator_generation: u64,
    occurrence_allocator_high_water: u64,
    global_root: Option<ArenaId>,
    directory_root: Option<ArenaId>,
    checkpoint_root: ArenaId,
    interner_root: ArenaId,
    source_lineage_root: ArenaId,
    green_root: ArenaId,
    occurrence_allocator_root: ArenaId,
    adoption_root: Option<ArenaId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SuffixAdoptionManifest {
    donor_source_revision: u64,
    candidate_source_revision: u64,
    old_suffix_start: u64,
    old_suffix_end: u64,
    new_suffix_start: u64,
    source_lineage_generation: u64,
    donor_range_root: ArenaId,
    source_lineage_root: ArenaId,
    checkpoint_green_root: ArenaId,
    candidate_green_root: ArenaId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DonorRangeManifest {
    donor_document_root: ArenaId,
    donor_global_root: Option<ArenaId>,
    donor_directory_root: Option<ArenaId>,
    donor_checkpoint_root: ArenaId,
    donor_interner_root: ArenaId,
    donor_green_root: ArenaId,
    donor_occurrence_allocator_root: ArenaId,
    donor_source_revision: u64,
    old_range: Range<u64>,
    donor_occurrence_count: u64,
    interner_generation: u64,
    source_lineage_generation: u64,
    source_lineage_root: ArenaId,
    occurrence_allocator_generation: u64,
    occurrence_allocator_high_water: u64,
}

fn encode_checkpoint_manifest(
    source_revision: u64,
    occurrence_high_water: u64,
    interner_generation: u64,
    source_lineage_generation: u64,
    has_prefix_directory: bool,
) -> [u8; CHECKPOINT_BYTES] {
    let mut payload = [0_u8; CHECKPOINT_BYTES];
    payload[0] = CHECKPOINT_TAG;
    payload[1] = FORMAT_VERSION;
    payload[2] = u8::from(has_prefix_directory);
    payload[3] = if has_prefix_directory { 2 } else { 1 };
    put_u64(&mut payload, 8, source_revision);
    put_u64(&mut payload, 16, occurrence_high_water);
    put_u64(&mut payload, 24, interner_generation);
    put_u64(&mut payload, 32, source_lineage_generation);
    payload
}

fn decode_checkpoint_manifest(
    arena: &PageArena,
    root: ArenaId,
) -> Result<CheckpointManifest, RestartIndexError> {
    let payload = arena.payload(root)?;
    if payload.len() != CHECKPOINT_BYTES
        || payload[0] != CHECKPOINT_TAG
        || payload[1] != FORMAT_VERSION
    {
        return Err(RestartIndexError::Invalid(
            "invalid restart checkpoint manifest",
        ));
    }
    let has_prefix = payload[2] != 0;
    let expected_children = usize::from(has_prefix) + 1;
    if usize::from(payload[3]) != expected_children
        || arena.packed_child_count(root)? != expected_children
    {
        return Err(RestartIndexError::Invalid(
            "checkpoint manifest child shape is invalid",
        ));
    }
    let prefix_directory = has_prefix
        .then(|| arena.packed_child_at(root, 0))
        .transpose()?;
    let green_root = arena.packed_child_at(root, usize::from(has_prefix))?;
    Ok(CheckpointManifest {
        source_revision: get_u64(payload, 8),
        occurrence_high_water: get_u64(payload, 16),
        interner_generation: get_u64(payload, 24),
        source_lineage_generation: get_u64(payload, 32),
        prefix_directory,
        green_root,
    })
}

#[allow(clippy::too_many_arguments)]
fn encode_document_manifest(
    source_revision: u64,
    parent_source_revision: u64,
    occurrence_count: u64,
    restart_high_water: u64,
    old_join_high_water: u64,
    replacement_count: u64,
    interner_generation: u64,
    source_lineage_generation: u64,
    occurrence_allocator_generation: u64,
    occurrence_allocator_high_water: u64,
    source_snapshot: Option<SourceSnapshotDescriptor>,
    has_global: bool,
    has_directory: bool,
    has_adoption: bool,
) -> Result<[u8; DOCUMENT_BYTES], RestartIndexError> {
    if source_snapshot
        .is_some_and(|source| source.revision.0 != source_revision || source.root.0 == 0)
    {
        return Err(RestartIndexError::Invalid(
            "document source snapshot is incomplete or crossed",
        ));
    }
    let mut payload = [0_u8; DOCUMENT_BYTES];
    payload[0] = DOCUMENT_TAG;
    payload[1] = FORMAT_VERSION;
    payload[2] = u8::from(has_global);
    payload[3] = u8::from(has_directory);
    payload[4] = u8::try_from(
        DOCUMENT_BASE_CHILDREN - usize::from(!has_global) - usize::from(!has_directory)
            + usize::from(has_adoption),
    )
    .expect("document child count fits u8");
    payload[5] = u8::from(has_adoption);
    payload[6] = u8::from(source_snapshot.is_some());
    put_u64(&mut payload, 8, source_revision);
    put_u64(&mut payload, 16, parent_source_revision);
    put_u64(&mut payload, 24, occurrence_count);
    put_u64(&mut payload, 32, restart_high_water);
    put_u64(&mut payload, 40, old_join_high_water);
    put_u64(&mut payload, 48, interner_generation);
    put_u64(&mut payload, 56, source_lineage_generation);
    put_u64(&mut payload, 64, replacement_count);
    put_u64(&mut payload, 72, occurrence_allocator_generation);
    put_u64(&mut payload, 80, occurrence_allocator_high_water);
    if let Some(source) = source_snapshot {
        put_u64(&mut payload, 88, source.root.0);
        put_u64(
            &mut payload,
            96,
            u64::try_from(source.bytes)
                .map_err(|_| RestartIndexError::Invalid("source byte extent exceeds u64"))?,
        );
    }
    Ok(payload)
}

fn decode_document_manifest(
    arena: &PageArena,
    root: ArenaId,
) -> Result<DocumentManifest, RestartIndexError> {
    let payload = arena.payload(root)?;
    if payload.len() != DOCUMENT_BYTES
        || payload[0] != DOCUMENT_TAG
        || payload[1] != FORMAT_VERSION
        || payload[6] > 1
    {
        return Err(RestartIndexError::Invalid(
            "invalid restart-index document manifest",
        ));
    }
    let has_global = payload[2] != 0;
    let has_directory = payload[3] != 0;
    let has_adoption = payload[5] != 0;
    let expected = DOCUMENT_BASE_CHILDREN - usize::from(!has_global) - usize::from(!has_directory)
        + usize::from(has_adoption);
    if usize::from(payload[4]) != expected || arena.packed_child_count(root)? != expected {
        return Err(RestartIndexError::Invalid(
            "restart-index document child shape is invalid",
        ));
    }
    let mut cursor = 0_usize;
    let global_root = if has_global {
        let child = arena.packed_child_at(root, cursor)?;
        cursor += 1;
        Some(child)
    } else {
        None
    };
    let directory_root = if has_directory {
        let child = arena.packed_child_at(root, cursor)?;
        cursor += 1;
        Some(child)
    } else {
        None
    };
    let checkpoint_root = arena.packed_child_at(root, cursor)?;
    let interner_root = arena.packed_child_at(root, cursor + 1)?;
    let source_lineage_root = arena.packed_child_at(root, cursor + 2)?;
    let green_root = arena.packed_child_at(root, cursor + 3)?;
    let occurrence_allocator_root = arena.packed_child_at(root, cursor + 4)?;
    let adoption_root = has_adoption
        .then(|| arena.packed_child_at(root, cursor + 5))
        .transpose()?;
    let source_revision = get_u64(payload, 8);
    let (lineage_generation, lineage_source) =
        decode_source_lineage_authority(arena, source_lineage_root)?;
    let has_source_snapshot = payload[6] != 0;
    let encoded_source_root = SourceRootId(get_u64(payload, 88));
    let encoded_source_bytes = usize::try_from(get_u64(payload, 96))
        .map_err(|_| RestartIndexError::Invalid("document source byte extent exceeds usize"))?;
    if !has_source_snapshot && (encoded_source_root.0 != 0 || encoded_source_bytes != 0) {
        return Err(RestartIndexError::Invalid(
            "legacy document carries unflagged source snapshot fields",
        ));
    }
    let source_snapshot = has_source_snapshot.then_some(SourceSnapshotDescriptor {
        revision: SourceRevision(source_revision),
        root: encoded_source_root,
        bytes: encoded_source_bytes,
    });
    if source_snapshot.is_some_and(|source| source.root.0 == 0)
        || lineage_source != source_snapshot.map(|source| (source.root, source.bytes))
    {
        return Err(RestartIndexError::Invalid(
            "document source snapshot crossed its lineage authority",
        ));
    }
    let manifest = DocumentManifest {
        source_revision,
        source_snapshot,
        parent_source_revision: get_u64(payload, 16),
        occurrence_count: get_u64(payload, 24),
        restart_high_water: get_u64(payload, 32),
        old_join_high_water: get_u64(payload, 40),
        replacement_count: get_u64(payload, 64),
        interner_generation: get_u64(payload, 48),
        source_lineage_generation: get_u64(payload, 56),
        occurrence_allocator_generation: get_u64(payload, 72),
        occurrence_allocator_high_water: get_u64(payload, 80),
        global_root,
        directory_root,
        checkpoint_root,
        interner_root,
        source_lineage_root,
        green_root,
        occurrence_allocator_root,
        adoption_root,
    };
    if sequence_length::<GlobalOccurrenceSpec>(arena, manifest.global_root)?
        != manifest.occurrence_count
    {
        return Err(RestartIndexError::Invalid(
            "document occurrence count disagrees with global sequence",
        ));
    }
    let allocator = decode_occurrence_allocator(arena, manifest.occurrence_allocator_root)?;
    if allocator
        != (
            manifest.occurrence_allocator_generation,
            manifest.occurrence_allocator_high_water,
        )
    {
        return Err(RestartIndexError::Invalid(
            "document occurrence allocator fields disagree with their root",
        ));
    }
    let checkpoint = decode_checkpoint_manifest(arena, manifest.checkpoint_root)?;
    let green = decode_green(arena, manifest.green_root)?;
    if checkpoint.source_revision != manifest.source_revision
        || checkpoint.occurrence_high_water != manifest.restart_high_water
        || checkpoint.interner_generation != manifest.interner_generation
        || checkpoint.source_lineage_generation != manifest.source_lineage_generation
        || checkpoint.green_root != manifest.green_root
        || green.0 != manifest.source_revision
        || decode_authority(arena, manifest.interner_root, 1)? != manifest.interner_generation
        || lineage_generation != manifest.source_lineage_generation
    {
        return Err(RestartIndexError::Invalid(
            "document checkpoint/green/interner/source lineage authority is crossed",
        ));
    }
    if manifest.parent_source_revision > manifest.source_revision {
        return Err(RestartIndexError::Invalid(
            "document source revision regressed behind its parent",
        ));
    }
    if manifest.parent_source_revision == manifest.source_revision {
        if manifest.adoption_root.is_some() || manifest.replacement_count != 0 {
            return Err(RestartIndexError::Invalid(
                "initial document carries a synthetic suffix adoption",
            ));
        }
    } else {
        let adoption_root = manifest.adoption_root.ok_or(RestartIndexError::Invalid(
            "candidate document omitted its suffix-adoption capability",
        ))?;
        let adoption = decode_suffix_adoption(arena, adoption_root)?;
        let donor = decode_donor_range(arena, adoption.donor_range_root)?;
        let deleted = manifest
            .old_join_high_water
            .checked_sub(manifest.restart_high_water)
            .ok_or(RestartIndexError::Invalid(
                "candidate restart range is reversed",
            ))?;
        let old_count = manifest
            .occurrence_count
            .checked_sub(manifest.replacement_count)
            .and_then(|count| count.checked_add(deleted))
            .ok_or(RestartIndexError::Invalid("candidate donor count overflow"))?;
        let new_suffix_start = manifest
            .restart_high_water
            .checked_add(manifest.replacement_count)
            .ok_or(RestartIndexError::Invalid(
                "candidate suffix start overflow",
            ))?;
        let expected_allocator_generation =
            donor.occurrence_allocator_generation.checked_add(1).ok_or(
                RestartIndexError::Invalid("candidate allocator generation overflow"),
            )?;
        let expected_allocator_high_water = donor
            .occurrence_allocator_high_water
            .checked_add(manifest.replacement_count)
            .ok_or(RestartIndexError::Invalid(
                "candidate allocator high-water overflow",
            ))?;
        let common_crossed = adoption.donor_source_revision != manifest.parent_source_revision
            || adoption.candidate_source_revision != manifest.source_revision
            || adoption.old_suffix_start != manifest.old_join_high_water
            || adoption.old_suffix_end != old_count
            || adoption.new_suffix_start != new_suffix_start
            || adoption.source_lineage_generation != manifest.source_lineage_generation
            || adoption.source_lineage_root != manifest.source_lineage_root
            || adoption.checkpoint_green_root != donor.donor_green_root
            || adoption.candidate_green_root != manifest.green_root
            || donor.donor_source_revision != manifest.parent_source_revision
            || donor.old_range != (manifest.restart_high_water..manifest.old_join_high_water)
            || donor.donor_occurrence_count != old_count
            || manifest.occurrence_allocator_generation != expected_allocator_generation
            || manifest.occurrence_allocator_high_water != expected_allocator_high_water;
        let lineage_crossed = if manifest.source_snapshot.is_some() {
            donor.source_lineage_generation >= manifest.source_lineage_generation
                || donor.source_lineage_root == manifest.source_lineage_root
        } else {
            donor.source_lineage_generation != manifest.source_lineage_generation
                || donor.source_lineage_root != manifest.source_lineage_root
        };
        let interner_crossed = if manifest.source_snapshot.is_some() {
            let candidate_interner = committed_interner_from_authority_root(
                arena,
                manifest.interner_root,
                manifest.interner_generation,
            )?;
            let donor_interner = committed_interner_from_authority_root(
                arena,
                donor.donor_interner_root,
                donor.interner_generation,
            )?;
            !candidate_interner.is_direct_adoption_of(arena, donor_interner)?
        } else {
            donor.interner_generation != manifest.interner_generation
                || donor.donor_interner_root != manifest.interner_root
        };
        if common_crossed || lineage_crossed || interner_crossed {
            return Err(RestartIndexError::Invalid(
                "suffix adoption does not bind donor range to candidate lineage",
            ));
        }
    }
    Ok(manifest)
}

fn encode_suffix_adoption(
    donor_source_revision: u64,
    candidate_source_revision: u64,
    old_suffix: Range<u64>,
    new_suffix_start: u64,
    source_lineage_generation: u64,
) -> [u8; SUFFIX_ADOPTION_BYTES] {
    let mut payload = [0_u8; SUFFIX_ADOPTION_BYTES];
    payload[0] = SUFFIX_ADOPTION_TAG;
    payload[1] = FORMAT_VERSION;
    payload[2] = 4;
    put_u64(&mut payload, 8, donor_source_revision);
    put_u64(&mut payload, 16, candidate_source_revision);
    put_u64(&mut payload, 24, old_suffix.start);
    put_u64(&mut payload, 32, old_suffix.end);
    put_u64(&mut payload, 40, new_suffix_start);
    put_u64(&mut payload, 48, source_lineage_generation);
    payload
}

fn decode_suffix_adoption(
    arena: &PageArena,
    root: ArenaId,
) -> Result<SuffixAdoptionManifest, RestartIndexError> {
    let payload = arena.payload(root)?;
    if payload.len() != SUFFIX_ADOPTION_BYTES
        || payload[0] != SUFFIX_ADOPTION_TAG
        || payload[1] != FORMAT_VERSION
        || payload[2] != 4
        || arena.packed_child_count(root)? != 4
    {
        return Err(RestartIndexError::Invalid(
            "invalid authenticated suffix-adoption capability",
        ));
    }
    let adoption = SuffixAdoptionManifest {
        donor_source_revision: get_u64(payload, 8),
        candidate_source_revision: get_u64(payload, 16),
        old_suffix_start: get_u64(payload, 24),
        old_suffix_end: get_u64(payload, 32),
        new_suffix_start: get_u64(payload, 40),
        source_lineage_generation: get_u64(payload, 48),
        donor_range_root: arena.packed_child_at(root, 0)?,
        source_lineage_root: arena.packed_child_at(root, 1)?,
        checkpoint_green_root: arena.packed_child_at(root, 2)?,
        candidate_green_root: arena.packed_child_at(root, 3)?,
    };
    if adoption.old_suffix_start > adoption.old_suffix_end
        || adoption.source_lineage_generation == 0
    {
        return Err(RestartIndexError::Invalid(
            "corrupt authenticated suffix-adoption coordinates",
        ));
    }
    Ok(adoption)
}

fn encode_donor_range(manifest: &DonorRangeManifest) -> [u8; DONOR_RANGE_BYTES] {
    let mut payload = [0_u8; DONOR_RANGE_BYTES];
    payload[0] = DONOR_RANGE_TAG;
    payload[1] = FORMAT_VERSION;
    payload[2] = u8::from(manifest.donor_global_root.is_some());
    payload[3] = u8::from(manifest.donor_directory_root.is_some());
    payload[4] = 5;
    put_arena_id(&mut payload, 8, manifest.donor_document_root);
    put_arena_id(
        &mut payload,
        16,
        manifest.donor_global_root.unwrap_or_default(),
    );
    put_arena_id(
        &mut payload,
        24,
        manifest.donor_directory_root.unwrap_or_default(),
    );
    put_arena_id(&mut payload, 32, manifest.donor_checkpoint_root);
    put_arena_id(&mut payload, 40, manifest.donor_interner_root);
    put_arena_id(&mut payload, 48, manifest.source_lineage_root);
    put_arena_id(&mut payload, 56, manifest.donor_green_root);
    put_arena_id(&mut payload, 64, manifest.donor_occurrence_allocator_root);
    put_u64(&mut payload, 72, manifest.donor_source_revision);
    put_u64(&mut payload, 80, manifest.old_range.start);
    put_u64(&mut payload, 88, manifest.old_range.end);
    put_u64(&mut payload, 96, manifest.donor_occurrence_count);
    put_u64(&mut payload, 104, manifest.interner_generation);
    put_u64(&mut payload, 112, manifest.source_lineage_generation);
    put_u64(&mut payload, 120, manifest.occurrence_allocator_generation);
    put_u64(&mut payload, 128, manifest.occurrence_allocator_high_water);
    // Legacy restart-proof marker. This manifest authenticates the retained
    // donor topology used by the fixture lane; it is not a production source
    // coordinate or cooked-value restart capability.
    put_u64(&mut payload, 136, 1);
    payload
}

fn decode_donor_range(
    arena: &PageArena,
    root: ArenaId,
) -> Result<DonorRangeManifest, RestartIndexError> {
    let payload = arena.payload(root)?;
    if payload.len() != DONOR_RANGE_BYTES
        || payload[0] != DONOR_RANGE_TAG
        || payload[1] != FORMAT_VERSION
        || payload[4] != 5
        || arena.packed_child_count(root)? != 5
        || get_u64(payload, 136) != 1
    {
        return Err(RestartIndexError::Invalid(
            "invalid authenticated donor semantic/source/projection range",
        ));
    }
    let has_global = payload[2] != 0;
    let has_directory = payload[3] != 0;
    let manifest = DonorRangeManifest {
        donor_document_root: get_arena_id(payload, 8),
        donor_global_root: has_global.then(|| get_arena_id(payload, 16)),
        donor_directory_root: has_directory.then(|| get_arena_id(payload, 24)),
        donor_checkpoint_root: get_arena_id(payload, 32),
        donor_interner_root: get_arena_id(payload, 40),
        source_lineage_root: get_arena_id(payload, 48),
        donor_green_root: get_arena_id(payload, 56),
        donor_occurrence_allocator_root: get_arena_id(payload, 64),
        donor_source_revision: get_u64(payload, 72),
        old_range: get_u64(payload, 80)..get_u64(payload, 88),
        donor_occurrence_count: get_u64(payload, 96),
        interner_generation: get_u64(payload, 104),
        source_lineage_generation: get_u64(payload, 112),
        occurrence_allocator_generation: get_u64(payload, 120),
        occurrence_allocator_high_water: get_u64(payload, 128),
    };
    let children = [
        manifest.donor_checkpoint_root,
        manifest.donor_interner_root,
        manifest.source_lineage_root,
        manifest.donor_green_root,
        manifest.donor_occurrence_allocator_root,
    ];
    if manifest.old_range.start > manifest.old_range.end
        || manifest.old_range.end > manifest.donor_occurrence_count
        || manifest.interner_generation == 0
        || manifest.source_lineage_generation == 0
        || manifest.occurrence_allocator_generation == 0
        || children
            .into_iter()
            .enumerate()
            .any(|(index, child)| arena.packed_child_at(root, index).ok() != Some(child))
    {
        return Err(RestartIndexError::Invalid(
            "crossed authenticated donor semantic/source/projection range",
        ));
    }
    Ok(manifest)
}

fn encode_authority(kind: u8, generation: u64) -> [u8; AUTHORITY_BYTES] {
    let mut payload = [0_u8; AUTHORITY_BYTES];
    payload[0] = AUTHORITY_TAG;
    payload[1] = FORMAT_VERSION;
    payload[2] = kind;
    put_u64(&mut payload, 8, generation);
    payload
}

fn encode_source_lineage_authority(
    generation: u64,
    source: SourceSnapshotDescriptor,
) -> Result<[u8; AUTHORITY_BYTES], RestartIndexError> {
    if generation == 0 || source.root.0 == 0 {
        return Err(RestartIndexError::Invalid(
            "source-lineage snapshot identity is incomplete",
        ));
    }
    let mut payload = encode_authority(2, generation);
    payload[3] = 1;
    put_u64(&mut payload, 16, source.root.0);
    put_u64(
        &mut payload,
        24,
        u64::try_from(source.bytes)
            .map_err(|_| RestartIndexError::Invalid("source byte extent exceeds u64"))?,
    );
    Ok(payload)
}

fn decode_source_lineage_authority(
    arena: &PageArena,
    root: ArenaId,
) -> Result<(u64, Option<(SourceRootId, usize)>), RestartIndexError> {
    let payload = arena.payload(root)?;
    if payload.first().copied() != Some(AUTHORITY_TAG)
        || payload.get(1).copied() != Some(FORMAT_VERSION)
        || payload.get(2).copied() != Some(2)
        || arena.packed_child_count(root)? != 0
    {
        return Err(RestartIndexError::Invalid(
            "invalid source-lineage authority root",
        ));
    }
    let (generation, source) = match (payload.len(), payload.get(3).copied()) {
        (AUTHORITY_BYTES, Some(0)) => (get_u64(payload, 8), None),
        (AUTHORITY_BYTES, Some(1)) => {
            let source_root = get_u64(payload, 16);
            let bytes = usize::try_from(get_u64(payload, 24)).map_err(|_| {
                RestartIndexError::Invalid("persisted source byte extent exceeds usize")
            })?;
            if source_root == 0 {
                return Err(RestartIndexError::Invalid(
                    "source-lineage snapshot identity is incomplete",
                ));
            }
            (
                get_u64(payload, 8),
                Some((SourceRootId(source_root), bytes)),
            )
        }
        _ => {
            return Err(RestartIndexError::Invalid(
                "invalid source-lineage authority encoding",
            ));
        }
    };
    if generation == 0 {
        return Err(RestartIndexError::Invalid(
            "source-lineage generation is zero",
        ));
    }
    Ok((generation, source))
}

fn decode_authority(
    arena: &PageArena,
    root: ArenaId,
    expected_kind: u8,
) -> Result<u64, RestartIndexError> {
    if expected_kind == 1 {
        return Ok(decode_interner_authority(arena, root)?.0);
    }
    if expected_kind == 2 {
        return Ok(decode_source_lineage_authority(arena, root)?.0);
    }
    let payload = arena.payload(root)?;
    if payload.len() != AUTHORITY_BYTES
        || payload[0] != AUTHORITY_TAG
        || payload[1] != FORMAT_VERSION
        || payload[2] != expected_kind
        || arena.packed_child_count(root)? != 0
    {
        return Err(RestartIndexError::Invalid("invalid exact authority root"));
    }
    let generation = get_u64(payload, 8);
    if generation == 0 {
        return Err(RestartIndexError::Invalid(
            "exact authority generation is zero",
        ));
    }
    Ok(generation)
}

fn encode_occurrence_allocator(generation: u64, high_water: u64) -> [u8; AUTHORITY_BYTES] {
    let mut payload = encode_authority(3, generation);
    put_u64(&mut payload, 16, high_water);
    payload
}

fn decode_occurrence_allocator(
    arena: &PageArena,
    root: ArenaId,
) -> Result<(u64, u64), RestartIndexError> {
    let payload = arena.payload(root)?;
    if payload.len() != AUTHORITY_BYTES
        || payload[0] != AUTHORITY_TAG
        || payload[1] != FORMAT_VERSION
        || payload[2] != 3
        || arena.packed_child_count(root)? != 0
    {
        return Err(RestartIndexError::Invalid(
            "invalid occurrence identity allocator root",
        ));
    }
    let generation = get_u64(payload, 8);
    let high_water = get_u64(payload, 16);
    if generation == 0 {
        return Err(RestartIndexError::Invalid(
            "occurrence identity allocator generation is zero",
        ));
    }
    Ok((generation, high_water))
}

fn encode_green(source_revision: u64, generation: u64, authority_nonce: u64) -> [u8; GREEN_BYTES] {
    let mut payload = [0_u8; GREEN_BYTES];
    payload[0] = GREEN_TAG;
    payload[1] = FORMAT_VERSION;
    put_u64(&mut payload, 8, source_revision);
    put_u64(&mut payload, 16, generation);
    put_u64(&mut payload, 24, authority_nonce);
    payload
}

fn decode_green(arena: &PageArena, root: ArenaId) -> Result<(u64, u64, u64), RestartIndexError> {
    let payload = arena.payload(root)?;
    if payload.len() != GREEN_BYTES
        || payload[0] != GREEN_TAG
        || payload[1] != FORMAT_VERSION
        || arena.packed_child_count(root)? != 0
    {
        return Err(RestartIndexError::Invalid(
            "invalid parser green authority root",
        ));
    }
    let decoded = (
        get_u64(payload, 8),
        get_u64(payload, 16),
        get_u64(payload, 24),
    );
    if decoded.1 == 0 || decoded.2 == 0 {
        return Err(RestartIndexError::Invalid(
            "parser green authority is incomplete",
        ));
    }
    Ok(decoded)
}

fn encode_interner_authority(
    generation: u64,
    label_high_water: u64,
    has_trie_root: bool,
) -> [u8; AUTHORITY_BYTES] {
    let mut payload = encode_authority(1, generation);
    payload[3] = u8::from(has_trie_root);
    put_u64(&mut payload, 16, label_high_water);
    payload
}

fn decode_interner_authority(
    arena: &PageArena,
    root: ArenaId,
) -> Result<(u64, u64, Option<ArenaId>), RestartIndexError> {
    let payload = arena.payload(root)?;
    if payload.len() != AUTHORITY_BYTES
        || payload[0] != AUTHORITY_TAG
        || payload[1] != FORMAT_VERSION
        || payload[2] != 1
        || payload[3] > 1
    {
        return Err(RestartIndexError::Invalid(
            "invalid exact reference-label interner authority",
        ));
    }
    let generation = get_u64(payload, 8);
    let label_high_water = get_u64(payload, 16);
    let has_root = payload[3] != 0;
    if generation == 0 || usize::from(has_root) != arena.packed_child_count(root)? {
        return Err(RestartIndexError::Invalid(
            "reference-label interner authority is incomplete",
        ));
    }
    let interner_root = has_root
        .then(|| arena.packed_child_at(root, 0))
        .transpose()?;
    Ok((generation, label_high_water, interner_root))
}

/// One-shot permission for the semantic index to consume an exact label from
/// the collision-proof interner. Only this module can construct the mint; the
/// interner consumes it and returns an acknowledgement that is withheld until
/// both ordered occurrence roots have advanced.
#[derive(Debug)]
pub(crate) struct ReferenceIndexInternerMint(());

impl ReferenceIndexInternerMint {
    const fn new() -> Self {
        Self(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReferenceCandidateIndexReceipt {
    bounded_tasks: u64,
    occurrences_acknowledged: u64,
    exact_labels: u64,
    checkpoint_occurrences: u64,
    maximum_pending_occurrences: usize,
    document_sized_occurrence_vectors: usize,
    label_grouping_maps: usize,
    /// Explicit HOLD: initial interning is proven, but authenticated restart
    /// adoption of an existing exact-label manifest is not yet wired.
    restart_interner_adoption_proven: bool,
    /// Explicit HOLD: no committed normalized-label-to-ID lookup capability
    /// is wired for production inline consumers yet.
    committed_exact_label_lookup_proven: bool,
    /// Explicit HOLD: the historical restart lane still collects its changed
    /// interval in `Vec`s rather than streaming through a persistent spool.
    restart_changed_interval_streaming_proven: bool,
    sequence_branches_allocated: usize,
    maximum_branches_per_task: usize,
    query_sequence_nodes_visited: u64,
    maximum_query_sequence_nodes_per_task: u64,
    maximum_live_owners: usize,
    manifest_children_joined: usize,
}

impl ReferenceCandidateIndexReceipt {
    pub(crate) const fn bounded_tasks(self) -> u64 {
        self.bounded_tasks
    }

    pub(crate) const fn occurrences_acknowledged(self) -> u64 {
        self.occurrences_acknowledged
    }

    pub(crate) const fn exact_labels(self) -> u64 {
        self.exact_labels
    }

    pub(crate) const fn checkpoint_occurrences(self) -> u64 {
        self.checkpoint_occurrences
    }

    pub(crate) const fn maximum_branches_per_task(self) -> usize {
        self.maximum_branches_per_task
    }

    pub(crate) const fn maximum_query_sequence_nodes_per_task(self) -> u64 {
        self.maximum_query_sequence_nodes_per_task
    }

    pub(crate) const fn maximum_pending_occurrences(self) -> usize {
        self.maximum_pending_occurrences
    }

    pub(crate) const fn document_sized_occurrence_vectors(self) -> usize {
        self.document_sized_occurrence_vectors
    }

    pub(crate) const fn label_grouping_maps(self) -> usize {
        self.label_grouping_maps
    }

    pub(crate) const fn restart_interner_adoption_proven(self) -> bool {
        self.restart_interner_adoption_proven
    }

    pub(crate) const fn committed_exact_label_lookup_proven(self) -> bool {
        self.committed_exact_label_lookup_proven
    }

    pub(crate) const fn restart_changed_interval_streaming_proven(self) -> bool {
        self.restart_changed_interval_streaming_proven
    }
}

/// Opaque, linear writer output for one definition occurrence. The concrete
/// CandidateWriter adapter may construct this only after the exact DFA output
/// has been range-validated and its destination/title values have been cooked
/// into persistent blobs. No source cut, destination ID, or raw label ID is
/// accepted by the semantic builder.
#[derive(Debug)]
#[must_use = "a writer-authenticated reference occurrence must be published or discarded"]
pub(crate) struct WriterAuthenticatedReferenceOccurrence {
    label: InternedReferenceLabel,
    destination: PersistentByteBlob,
    title: Option<PersistentByteBlob>,
}

impl WriterAuthenticatedReferenceOccurrence {
    /// The only production construction seam: all inputs are already opaque,
    /// non-cloneable capabilities. CandidateWriter remains responsible for
    /// proving that these cooked blobs came from this exact parser output.
    pub(crate) const fn from_writer_cooked(
        label: InternedReferenceLabel,
        destination: PersistentByteBlob,
        title: Option<PersistentByteBlob>,
        _mint: &mut crate::candidate_writer::ReferenceCandidateIndexWriterMint,
    ) -> Self {
        Self {
            label,
            destination,
            title,
        }
    }

    #[cfg(test)]
    const fn proof_only(
        label: InternedReferenceLabel,
        destination: PersistentByteBlob,
        title: Option<PersistentByteBlob>,
    ) -> Self {
        Self {
            label,
            destination,
            title,
        }
    }
}

/// Returned only after both the global occurrence sequence and the matching
/// exact-label sequence have advanced. CandidateWriter passes the contained
/// capability back to the interner before requesting another label.
#[derive(Debug)]
#[must_use = "the exact-label interner must acknowledge this occurrence"]
pub(crate) struct ReferenceCandidateOccurrenceAck {
    label_use: ReferenceLabelUseAck,
}

impl ReferenceCandidateOccurrenceAck {
    pub(crate) fn into_label_use(self) -> ReferenceLabelUseAck {
        self.label_use
    }
}

fn push_sequence_leaf<Spec>(
    session: &mut ArenaBuildSession<'_>,
    builder: &mut ResumableStreamingSequenceBuilder<Spec>,
    leaf: ArenaBuildOwner,
    receipt: &mut SequenceMutationReceipt,
) -> Result<(), RestartIndexError>
where
    Spec: SequenceSpec<Error = RestartIndexError>,
{
    builder.begin_push(session, leaf, receipt)?;
    while builder.poll_push(session, receipt)? == ResumableSequenceProgress::Pending {}
    Ok(())
}

fn finish_sequence<Spec>(
    session: &mut ArenaBuildSession<'_>,
    builder: &mut ResumableStreamingSequenceBuilder<Spec>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<ArenaBuildOwner, RestartIndexError>
where
    Spec: SequenceSpec<Error = RestartIndexError>,
{
    builder.begin_finish(receipt)?;
    while builder.poll_finish(session, receipt)? == ResumableSequenceProgress::Pending {}
    builder.take_root()
}

fn drive_splice<Spec>(
    session: &mut ArenaBuildSession<'_>,
    splice: &mut ResumableSequenceSplice<Spec>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<ArenaBuildOwner>, RestartIndexError>
where
    Spec: SequenceSpec<Error = RestartIndexError>,
{
    while splice.poll(session, receipt)? == ResumableSequenceSplitProgress::Pending {}
    splice.take_root()
}

fn build_global_sequence(
    session: &mut ArenaBuildSession<'_>,
    occurrences: &[ReferenceOccurrence],
    receipt: &mut SequenceMutationReceipt,
) -> Result<(Option<ArenaBuildOwner>, Vec<ArenaId>), RestartIndexError> {
    if occurrences.is_empty() {
        return Ok((None, Vec::new()));
    }
    let mut builder = ResumableStreamingSequenceBuilder::<GlobalOccurrenceSpec>::try_new(receipt)?;
    let mut leaves = Vec::new();
    leaves
        .try_reserve_exact(occurrences.len())
        .map_err(|_| RestartIndexError::Invalid("global leaf-ID reservation failed"))?;
    for occurrence in occurrences.iter().copied() {
        occurrence.validate()?;
        let leaf = allocate_global_occurrence_leaf(session, occurrence)?;
        leaves.push(session.owner_id(&leaf)?);
        push_sequence_leaf(session, &mut builder, leaf, receipt)?;
    }
    Ok((
        Some(finish_sequence(session, &mut builder, receipt)?),
        leaves,
    ))
}

fn build_label_sequence(
    session: &mut ArenaBuildSession<'_>,
    occurrences: &[ReferenceOccurrence],
    descriptor_leaves: &[ArenaId],
    indexes: &[usize],
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<ArenaBuildOwner>, RestartIndexError> {
    if indexes.is_empty() {
        return Ok(None);
    }
    let mut builder = ResumableStreamingSequenceBuilder::<LabelOccurrenceSpec>::try_new(receipt)?;
    for &index in indexes {
        let occurrence = *occurrences.get(index).ok_or(RestartIndexError::Invalid(
            "per-label build index escaped global occurrences",
        ))?;
        let descriptor = *descriptor_leaves
            .get(index)
            .ok_or(RestartIndexError::Invalid(
                "per-label build lost global descriptor identity",
            ))?;
        let payload = encode_label_occurrence(occurrence);
        let (leaf, _) = session.allocate_packed(&payload, &[descriptor])?;
        push_sequence_leaf(session, &mut builder, leaf, receipt)?;
    }
    Ok(Some(finish_sequence(session, &mut builder, receipt)?))
}

fn concatenate_label_sequences(
    session: &mut ArenaBuildSession<'_>,
    prefix: Option<ArenaBuildOwner>,
    suffix: Option<ArenaBuildOwner>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<ArenaBuildOwner>, RestartIndexError> {
    match (prefix, suffix) {
        (None, none) | (none, None) => Ok(none),
        (Some(prefix), Some(suffix)) => {
            let prefix_id = session.owner_id(&prefix)?;
            let prefix_len =
                sequence_length::<LabelOccurrenceSpec>(session.arena(), Some(prefix_id))?;
            let mut splice = ResumableSequenceSplice::<LabelOccurrenceSpec>::try_from_owned(
                session,
                Some(prefix),
                prefix_len..prefix_len,
                Some(suffix),
                receipt,
            )?;
            drive_splice(session, &mut splice, receipt)
        }
    }
}

fn push_directory_entry(
    session: &mut ArenaBuildSession<'_>,
    builder: &mut ResumableStreamingSequenceBuilder<ExactLabelDirectorySpec>,
    label_id: u64,
    sequence: ArenaBuildOwner,
    receipt: &mut SequenceMutationReceipt,
) -> Result<(), RestartIndexError> {
    let sequence_id = session.owner_id(&sequence)?;
    let length = sequence_length::<LabelOccurrenceSpec>(session.arena(), Some(sequence_id))?;
    if length == 0 {
        return Err(RestartIndexError::Invalid(
            "empty per-label sequence must be absent from exact directory",
        ));
    }
    let payload = encode_directory_leaf(label_id, length);
    let (leaf, _) = session.allocate_packed(&payload, &[sequence_id])?;
    session.release(sequence)?;
    push_sequence_leaf(session, builder, leaf, receipt)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct InitialBuildReceipt {
    occurrences: u64,
    checkpoint_occurrences: u64,
    exact_labels: u64,
    sequence_branches_allocated: u64,
    append_splice_branches_allocated: u64,
    selected_checkpoint_root_edges: u64,
    selected_checkpoint_history_nodes_upper_bound: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct ReferenceCheckpointSeal {
    occurrence_high_water: u64,
}

impl ReferenceCheckpointSeal {
    fn try_new(
        occurrence_high_water: u64,
        pending_definition_ack: bool,
        active_paragraph: bool,
    ) -> Result<Self, RestartIndexError> {
        if pending_definition_ack || active_paragraph {
            return Err(RestartIndexError::Invalid(
                "reference checkpoint requires every definition ack and no active Paragraph",
            ));
        }
        Ok(Self {
            occurrence_high_water,
        })
    }
}

#[derive(Debug)]
struct RestartIndexDocument {
    owner: Option<OwnedArenaRef>,
    checkpoint: ArenaScopedId,
    initial_receipt: InitialBuildReceipt,
}

impl RestartIndexDocument {
    fn root(&self) -> Result<ArenaScopedId, RestartIndexError> {
        self.owner
            .as_ref()
            .map(OwnedArenaRef::scoped_id)
            .ok_or(RestartIndexError::Invalid(
                "restart-index document owner was released",
            ))
    }

    const fn checkpoint(&self) -> ArenaScopedId {
        self.checkpoint
    }

    const fn initial_receipt(&self) -> InitialBuildReceipt {
        self.initial_receipt
    }

    fn manifest(&self, arena: &PageArena) -> Result<DocumentManifest, RestartIndexError> {
        decode_document_manifest(arena, arena.local_id(self.root()?)?)
    }

    fn occurrence_at(
        &self,
        arena: &PageArena,
        index: u64,
    ) -> Result<(ArenaId, ReferenceOccurrence), RestartIndexError> {
        let manifest = self.manifest(arena)?;
        let value = global_occurrence_at(arena, manifest.global_root, index)?;
        validate_occurrence_generation(manifest, index, value.1)?;
        Ok(value)
    }

    fn winner(
        &self,
        arena: &PageArena,
        label_id: u64,
    ) -> Result<Option<(ArenaId, ArenaId, ReferenceOccurrence)>, RestartIndexError> {
        let manifest = self.manifest(arena)?;
        let lookup = lookup_directory(arena, manifest.directory_root, label_id)?;
        if lookup.leaf.is_none() {
            return Ok(None);
        }
        let winner = per_label_occurrence_at(arena, lookup.sequence_root, 0)?;
        validate_unpositioned_occurrence_generation(manifest, winner.2)?;
        Ok(Some(winner))
    }

    fn label_occurrences(
        &self,
        arena: &PageArena,
        label_id: u64,
    ) -> Result<Vec<ReferenceOccurrence>, RestartIndexError> {
        let manifest = self.manifest(arena)?;
        let lookup = lookup_directory(arena, manifest.directory_root, label_id)?;
        let mut output = Vec::new();
        for index in 0..lookup.sequence_length {
            let occurrence = per_label_occurrence_at(arena, lookup.sequence_root, index)?.2;
            validate_unpositioned_occurrence_generation(manifest, occurrence)?;
            output.push(occurrence);
        }
        Ok(output)
    }

    fn release_later(mut self, arena: &mut PageArena) -> Result<(), RestartIndexError> {
        if let Some(owner) = self.owner.take() {
            arena
                .release_later(owner)
                .map_err(|failure| RestartIndexError::Arena(failure.error))?;
        }
        Ok(())
    }
}

/// Non-owning, arena-scoped view of one committed reference semantic root.
/// In production the composite document owns the root; this capability is
/// minted only by the CandidateWriter join and fails stale after that owner is
/// retired. It is the common authority for restart adoption and inline
/// normalized-label resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommittedReferenceIndex {
    root: ArenaScopedId,
    checkpoint: ArenaScopedId,
}

impl CommittedReferenceIndex {
    pub(crate) fn from_candidate_writer_join(
        arena: &PageArena,
        root: ArenaScopedId,
        checkpoint: ArenaScopedId,
        _mint: &mut crate::candidate_writer::ReferenceCandidateIndexWriterMint,
    ) -> Result<Self, RestartIndexError> {
        Self::validate_and_new(arena, root, checkpoint)
    }

    #[cfg(test)]
    fn proof_only(
        arena: &PageArena,
        root: ArenaScopedId,
        checkpoint: ArenaScopedId,
    ) -> Result<Self, RestartIndexError> {
        Self::validate_and_new(arena, root, checkpoint)
    }

    fn validate_and_new(
        arena: &PageArena,
        root: ArenaScopedId,
        checkpoint: ArenaScopedId,
    ) -> Result<Self, RestartIndexError> {
        let manifest = decode_document_manifest(arena, arena.local_id(root)?)?;
        if arena.local_id(checkpoint)? != manifest.checkpoint_root {
            return Err(RestartIndexError::Invalid(
                "committed reference checkpoint crossed its document",
            ));
        }
        Ok(Self { root, checkpoint })
    }

    fn manifest(self, arena: &PageArena) -> Result<DocumentManifest, RestartIndexError> {
        decode_document_manifest(arena, arena.local_id(self.root)?)
    }

    /// The immutable source snapshot authenticated by this committed semantic
    /// root. Composite parents use this typed seam to reject a crossed Green
    /// child without duplicating the private restart-index storage schema.
    pub(crate) fn source_snapshot(
        self,
        arena: &PageArena,
    ) -> Result<SourceSnapshotDescriptor, RestartIndexError> {
        self.manifest(arena)?
            .source_snapshot
            .ok_or(RestartIndexError::Invalid(
                "committed reference index omitted its source snapshot",
            ))
    }

    fn interner(
        self,
        arena: &PageArena,
    ) -> Result<CommittedReferenceLabelInterner, RestartIndexError> {
        let manifest = self.manifest(arena)?;
        committed_interner_from_manifest(arena, manifest)
    }

    pub(crate) fn begin_winner_query(
        self,
        arena: &PageArena,
        raw_label: &str,
    ) -> Result<CommittedReferenceWinnerQuery, RestartIndexError> {
        let normalized = if reference_label_length_is_valid(raw_label) {
            let normalized = normalize_reference_label(raw_label);
            (!normalized.is_empty()).then_some(normalized)
        } else {
            None
        };
        let ready_without_lookup = normalized.is_none();
        let lookup = match normalized {
            Some(normalized) => Some(
                self.interner(arena)?
                    .begin_lookup_normalized(arena, normalized)?,
            ),
            None => None,
        };
        Ok(CommittedReferenceWinnerQuery {
            document: self,
            lookup,
            result: None,
            ready_without_lookup,
            taken: false,
            receipt: CommittedReferenceWinnerQueryReceipt::default(),
        })
    }
}

fn committed_interner_from_manifest(
    arena: &PageArena,
    manifest: DocumentManifest,
) -> Result<CommittedReferenceLabelInterner, RestartIndexError> {
    committed_interner_from_authority_root(
        arena,
        manifest.interner_root,
        manifest.interner_generation,
    )
}

fn committed_interner_from_authority_root(
    arena: &PageArena,
    interner_root: ArenaId,
    expected_generation: u64,
) -> Result<CommittedReferenceLabelInterner, RestartIndexError> {
    let (generation, label_id_high_water, interner_manifest) =
        decode_interner_authority(arena, interner_root)?;
    if generation != expected_generation {
        return Err(RestartIndexError::Invalid(
            "committed document crossed its exact-label generation",
        ));
    }
    let interner_manifest = interner_manifest.ok_or(RestartIndexError::Invalid(
        "committed document omitted its exact-label manifest",
    ))?;
    CommittedReferenceLabelInterner::from_reference_index_join(
        arena,
        arena.scoped_query_id(interner_manifest)?,
        generation,
        label_id_high_water,
        &mut ReferenceIndexInternerMint::new(),
    )
    .map_err(Into::into)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommittedReferenceBlobView {
    root: Option<ArenaId>,
    bytes: u64,
    chunks: u64,
    height: u16,
}

impl CommittedReferenceBlobView {
    pub(crate) const fn bytes(self) -> u64 {
        self.bytes
    }

    pub(crate) const fn metadata(self) -> PersistentByteBlobMetadata {
        PersistentByteBlobMetadata {
            root: self.root,
            bytes: self.bytes,
            chunks: self.chunks,
            height: self.height,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommittedReferenceWinner {
    document_root: ArenaScopedId,
    occurrence_id: u64,
    label_id: u64,
    destination: CommittedReferenceBlobView,
    title: Option<CommittedReferenceBlobView>,
}

impl CommittedReferenceWinner {
    pub(crate) const fn occurrence_id(self) -> u64 {
        self.occurrence_id
    }

    pub(crate) const fn label_id(self) -> u64 {
        self.label_id
    }

    pub(crate) fn destination(
        self,
        arena: &PageArena,
    ) -> Result<CommittedReferenceBlobView, RestartIndexError> {
        arena.local_id(self.document_root)?;
        Ok(self.destination)
    }

    pub(crate) fn title(
        self,
        arena: &PageArena,
    ) -> Result<Option<CommittedReferenceBlobView>, RestartIndexError> {
        arena.local_id(self.document_root)?;
        Ok(self.title)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommittedReferenceWinnerQueryReceipt {
    polls: u64,
    exact_lookup_pages: u64,
    semantic_nodes_visited: u64,
    maximum_semantic_nodes_per_poll: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommittedReferenceWinnerQueryProgress {
    Pending,
    Ready,
}

#[derive(Debug)]
pub(crate) struct CommittedReferenceWinnerQuery {
    document: CommittedReferenceIndex,
    lookup: Option<CommittedReferenceLabelLookup>,
    result: Option<Option<CommittedReferenceWinner>>,
    ready_without_lookup: bool,
    taken: bool,
    receipt: CommittedReferenceWinnerQueryReceipt,
}

impl CommittedReferenceWinnerQuery {
    pub(crate) const fn receipt(&self) -> CommittedReferenceWinnerQueryReceipt {
        self.receipt
    }

    pub(crate) fn poll(
        &mut self,
        arena: &PageArena,
    ) -> Result<CommittedReferenceWinnerQueryProgress, RestartIndexError> {
        if self.taken {
            return Err(RestartIndexError::Invalid(
                "committed reference winner query was consumed",
            ));
        }
        arena.local_id(self.document.root)?;
        if self.result.is_some() {
            return Ok(CommittedReferenceWinnerQueryProgress::Ready);
        }
        self.receipt.polls =
            self.receipt
                .polls
                .checked_add(1)
                .ok_or(RestartIndexError::Invalid(
                    "committed winner query poll count overflow",
                ))?;
        if self.ready_without_lookup {
            self.result = Some(None);
            return Ok(CommittedReferenceWinnerQueryProgress::Ready);
        }
        let lookup = self.lookup.as_mut().ok_or(RestartIndexError::Invalid(
            "committed winner query lost its exact-label lookup",
        ))?;
        if lookup.poll(arena)? == CommittedReferenceLabelLookupProgress::Pending {
            self.receipt.exact_lookup_pages = lookup.receipt().pages_read;
            return Ok(CommittedReferenceWinnerQueryProgress::Pending);
        }
        self.receipt.exact_lookup_pages = lookup.receipt().pages_read;
        let Some(label_id) = lookup.take_label_id()? else {
            self.result = Some(None);
            return Ok(CommittedReferenceWinnerQueryProgress::Ready);
        };
        let manifest = self.document.manifest(arena)?;
        let mut query_receipt = QueryWorkReceipt::default();
        let directory =
            lookup_directory_counted(arena, manifest.directory_root, label_id, &mut query_receipt)?;
        let winner = if directory.leaf.is_some() {
            let occurrence = per_label_occurrence_at_counted(
                arena,
                directory.sequence_root,
                0,
                &mut query_receipt,
            )?
            .2;
            let ReferenceOccurrenceValue::Cooked(value) = occurrence.value else {
                return Err(RestartIndexError::Invalid(
                    "committed production query reached a fixture occurrence",
                ));
            };
            Some(CommittedReferenceWinner {
                document_root: self.document.root,
                occurrence_id: occurrence.occurrence_id,
                label_id,
                destination: committed_blob_view(
                    arena,
                    value.destination_root,
                    value.destination_bytes,
                )?,
                title: value
                    .title
                    .map(|title| committed_blob_view(arena, title.root, title.bytes))
                    .transpose()?,
            })
        } else {
            None
        };
        if query_receipt.sequence_nodes_visited > MAX_QUERY_SEQUENCE_NODES_PER_TASK {
            return Err(RestartIndexError::Invalid(
                "committed winner lookup exceeded its bounded semantic query envelope",
            ));
        }
        self.receipt.semantic_nodes_visited = self
            .receipt
            .semantic_nodes_visited
            .checked_add(query_receipt.sequence_nodes_visited)
            .ok_or(RestartIndexError::Invalid(
                "committed winner semantic query receipt overflow",
            ))?;
        self.receipt.maximum_semantic_nodes_per_poll = self
            .receipt
            .maximum_semantic_nodes_per_poll
            .max(query_receipt.sequence_nodes_visited);
        self.result = Some(winner);
        Ok(CommittedReferenceWinnerQueryProgress::Ready)
    }

    pub(crate) fn take(&mut self) -> Result<Option<CommittedReferenceWinner>, RestartIndexError> {
        if self.taken {
            return Err(RestartIndexError::Invalid(
                "committed reference winner query was already consumed",
            ));
        }
        let result = self.result.take().ok_or(RestartIndexError::Invalid(
            "committed reference winner query is not ready",
        ))?;
        self.taken = true;
        Ok(result)
    }
}

fn committed_blob_view(
    arena: &PageArena,
    root: Option<ArenaId>,
    bytes: u64,
) -> Result<CommittedReferenceBlobView, RestartIndexError> {
    let (chunks, height) = match root {
        Some(root) => {
            let summary = sequence_node::<PersistentBlobSpec>(arena, root)?.0;
            if summary.bytes != bytes {
                return Err(RestartIndexError::Invalid(
                    "committed cooked blob length crossed its root",
                ));
            }
            (summary.chunks, summary.height)
        }
        None if bytes == 0 => (0, 0),
        None => {
            return Err(RestartIndexError::Invalid(
                "committed cooked blob omitted a nonempty root",
            ));
        }
    };
    Ok(CommittedReferenceBlobView {
        root,
        bytes,
        chunks,
        height,
    })
}

fn validate_occurrence_generation(
    manifest: DocumentManifest,
    index: u64,
    occurrence: ReferenceOccurrence,
) -> Result<(), RestartIndexError> {
    match occurrence.value {
        ReferenceOccurrenceValue::Cooked(_) => Ok(()),
        #[cfg(test)]
        ReferenceOccurrenceValue::FixtureLegacy {
            coordinate_generation,
            ..
        } => {
            if manifest.parent_source_revision == manifest.source_revision {
                if coordinate_generation != manifest.source_revision {
                    return Err(RestartIndexError::Invalid(
                        "initial occurrence coordinate generation crossed its source",
                    ));
                }
            } else {
                let replacement_end = manifest
                    .restart_high_water
                    .checked_add(manifest.replacement_count)
                    .ok_or(RestartIndexError::Invalid(
                        "replacement coordinate overflow",
                    ))?;
                let accepted = if index >= manifest.restart_high_water && index < replacement_end {
                    coordinate_generation == manifest.source_revision
                } else {
                    // Prefix and suffix coordinates can predate the immediate donor
                    // after chained edits.  Their stable source-piece/projection IDs
                    // are authenticated by the retained lineage root and donor range.
                    coordinate_generation <= manifest.parent_source_revision
                        && manifest.adoption_root.is_some()
                };
                if !accepted {
                    return Err(RestartIndexError::Invalid(
                        "occurrence coordinate generation is not covered by replacement or lineage adoption",
                    ));
                }
            }
            Ok(())
        }
    }
}

fn validate_unpositioned_occurrence_generation(
    manifest: DocumentManifest,
    occurrence: ReferenceOccurrence,
) -> Result<(), RestartIndexError> {
    match occurrence.value {
        ReferenceOccurrenceValue::Cooked(_) => Ok(()),
        #[cfg(test)]
        ReferenceOccurrenceValue::FixtureLegacy {
            coordinate_generation,
            ..
        } => {
            let accepted = coordinate_generation == manifest.source_revision
                || (manifest.parent_source_revision != manifest.source_revision
                    && coordinate_generation <= manifest.parent_source_revision
                    && manifest.adoption_root.is_some());
            if !accepted {
                return Err(RestartIndexError::Invalid(
                    "per-label occurrence has no candidate or authenticated donor generation",
                ));
            }
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg(test)]
fn build_initial_document(
    arena: &mut PageArena,
    occurrences: Vec<ReferenceOccurrence>,
    checkpoint: ReferenceCheckpointSeal,
    source_revision: u64,
    interner_generation: u64,
    source_lineage_generation: u64,
) -> Result<RestartIndexDocument, RestartIndexError> {
    let checkpoint_high_water = checkpoint.occurrence_high_water;
    if interner_generation == 0
        || source_lineage_generation == 0
        || checkpoint_high_water > u64::try_from(occurrences.len()).unwrap_or(u64::MAX)
        || occurrences
            .iter()
            .any(|occurrence| occurrence.fixture_coordinate_generation() != Some(source_revision))
    {
        return Err(RestartIndexError::Invalid(
            "initial restart-index authority is incomplete or crossed",
        ));
    }
    let mut occurrence_ids = BTreeSet::new();
    let mut occurrence_allocator_high_water = 0_u64;
    let mut fixture_label_high_water = 0_u64;
    for occurrence in &occurrences {
        if !occurrence_ids.insert(occurrence.occurrence_id) {
            return Err(RestartIndexError::Invalid(
                "initial document reused an occurrence identity",
            ));
        }
        occurrence_allocator_high_water =
            occurrence_allocator_high_water.max(occurrence.occurrence_id);
        fixture_label_high_water = fixture_label_high_water.max(occurrence.label_id);
    }
    let ticket = arena.begin_build()?;
    let mut session = arena
        .resume_build(ticket)
        .map_err(|failure| RestartIndexError::ArenaBuild(failure.error))?;
    let mut sequence_receipt = SequenceMutationReceipt::default();
    let (global_root, descriptor_leaves) =
        build_global_sequence(&mut session, &occurrences, &mut sequence_receipt)?;

    // Fixture/bootstrap-only collection.  The restart job below never groups
    // a changed document; it performs one forward deletion and one reverse
    // insertion at a time with bounded scratch.
    let mut by_label = BTreeMap::<u64, Vec<usize>>::new();
    for (index, occurrence) in occurrences.iter().enumerate() {
        by_label.entry(occurrence.label_id).or_default().push(index);
    }

    let mut full_directory_builder = (!by_label.is_empty())
        .then(|| {
            ResumableStreamingSequenceBuilder::<ExactLabelDirectorySpec>::try_new(
                &mut sequence_receipt,
            )
        })
        .transpose()?;
    let prefix_label_count = by_label
        .values()
        .filter(|indexes| {
            indexes
                .first()
                .is_some_and(|index| u64::try_from(*index).unwrap() < checkpoint_high_water)
        })
        .count();
    let mut prefix_directory_builder = (prefix_label_count != 0)
        .then(|| {
            ResumableStreamingSequenceBuilder::<ExactLabelDirectorySpec>::try_new(
                &mut sequence_receipt,
            )
        })
        .transpose()?;

    let branches_before_label_appends = sequence_receipt.branches_allocated;
    let mut checkpoint_history_nodes_upper_bound = 0_u64;
    for (&label_id, indexes) in &by_label {
        let prefix_end = indexes.partition_point(|index| {
            u64::try_from(*index).expect("occurrence index fits u64") < checkpoint_high_water
        });
        let (prefix_indexes, suffix_indexes) = indexes.split_at(prefix_end);
        let prefix = build_label_sequence(
            &mut session,
            &occurrences,
            &descriptor_leaves,
            prefix_indexes,
            &mut sequence_receipt,
        )?;
        let (prefix_for_full, prefix_for_checkpoint) = if let Some(prefix) = prefix {
            let prefix_id = session.owner_id(&prefix)?;
            checkpoint_history_nodes_upper_bound = checkpoint_history_nodes_upper_bound
                .checked_add(u64::from(
                    sequence_node::<LabelOccurrenceSpec>(session.arena(), prefix_id)?
                        .0
                        .height,
                ))
                .ok_or(RestartIndexError::Invalid(
                    "checkpoint history bound overflow",
                ))?;
            (Some(prefix), Some(session.retain(prefix_id)?))
        } else {
            (None, None)
        };
        let suffix = build_label_sequence(
            &mut session,
            &occurrences,
            &descriptor_leaves,
            suffix_indexes,
            &mut sequence_receipt,
        )?;
        let full = concatenate_label_sequences(
            &mut session,
            prefix_for_full,
            suffix,
            &mut sequence_receipt,
        )?
        .ok_or(RestartIndexError::Invalid(
            "full label group unexpectedly became empty",
        ))?;
        push_directory_entry(
            &mut session,
            full_directory_builder
                .as_mut()
                .ok_or(RestartIndexError::Invalid(
                    "full label directory disappeared",
                ))?,
            label_id,
            full,
            &mut sequence_receipt,
        )?;
        if let Some(prefix) = prefix_for_checkpoint {
            push_directory_entry(
                &mut session,
                prefix_directory_builder
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "checkpoint prefix directory disappeared",
                    ))?,
                label_id,
                prefix,
                &mut sequence_receipt,
            )?;
        }
    }
    let append_splice_branches_allocated = sequence_receipt
        .branches_allocated
        .saturating_sub(branches_before_label_appends);
    let full_directory = full_directory_builder
        .as_mut()
        .map(|builder| finish_sequence(&mut session, builder, &mut sequence_receipt))
        .transpose()?;
    let prefix_directory = prefix_directory_builder
        .as_mut()
        .map(|builder| finish_sequence(&mut session, builder, &mut sequence_receipt))
        .transpose()?;

    // Fixture-only documents begin from already-numbered labels. Production
    // construction below owns a collision-proof trie and never accepts these
    // raw IDs at its boundary.
    let (interner, _) = session.allocate_packed(
        &encode_interner_authority(interner_generation, fixture_label_high_water, false),
        &[],
    )?;
    let interner_id = session.owner_id(&interner)?;
    let (source_lineage, _) =
        session.allocate_packed(&encode_authority(2, source_lineage_generation), &[])?;
    let source_lineage_id = session.owner_id(&source_lineage)?;
    let (green, _) = session.allocate_packed(&encode_green(source_revision, 1, 1), &[])?;
    let green_id = session.owner_id(&green)?;
    let (occurrence_allocator, _) = session.allocate_packed(
        &encode_occurrence_allocator(1, occurrence_allocator_high_water),
        &[],
    )?;
    let occurrence_allocator_id = session.owner_id(&occurrence_allocator)?;

    let mut checkpoint_children = Vec::with_capacity(2);
    if let Some(prefix) = prefix_directory.as_ref() {
        checkpoint_children.push(session.owner_id(prefix)?);
    }
    checkpoint_children.push(green_id);
    let checkpoint_payload = encode_checkpoint_manifest(
        source_revision,
        checkpoint_high_water,
        interner_generation,
        source_lineage_generation,
        prefix_directory.is_some(),
    );
    let (checkpoint, _) = session.allocate_packed(&checkpoint_payload, &checkpoint_children)?;
    let checkpoint_id = session.owner_id(&checkpoint)?;
    if let Some(prefix) = prefix_directory {
        session.release(prefix)?;
    }

    let mut document_children = Vec::with_capacity(DOCUMENT_BASE_CHILDREN);
    if let Some(global) = global_root.as_ref() {
        document_children.push(session.owner_id(global)?);
    }
    if let Some(directory) = full_directory.as_ref() {
        document_children.push(session.owner_id(directory)?);
    }
    document_children.extend([
        checkpoint_id,
        interner_id,
        source_lineage_id,
        green_id,
        occurrence_allocator_id,
    ]);
    let document_payload = encode_document_manifest(
        source_revision,
        source_revision,
        u64::try_from(occurrences.len())
            .map_err(|_| RestartIndexError::Invalid("initial occurrence count overflow"))?,
        checkpoint_high_water,
        u64::try_from(occurrences.len())
            .map_err(|_| RestartIndexError::Invalid("initial join high-water overflow"))?,
        0,
        interner_generation,
        source_lineage_generation,
        1,
        occurrence_allocator_high_water,
        None,
        global_root.is_some(),
        full_directory.is_some(),
        false,
    )?;
    let (document, _) = session.allocate_packed(&document_payload, &document_children)?;
    if let Some(global) = global_root {
        session.release(global)?;
    }
    if let Some(directory) = full_directory {
        session.release(directory)?;
    }
    session.release(checkpoint)?;
    session.release(interner)?;
    session.release(source_lineage)?;
    session.release(green)?;
    session.release(occurrence_allocator)?;
    if session.live_owners()? != 1 {
        return Err(RestartIndexError::Invalid(
            "initial build did not reduce to one composite manifest",
        ));
    }
    let owner = session.commit(document)?;
    let checkpoint = arena.scoped_query_id(checkpoint_id)?;
    Ok(RestartIndexDocument {
        owner: Some(owner),
        checkpoint,
        initial_receipt: InitialBuildReceipt {
            occurrences: u64::try_from(occurrences.len())
                .map_err(|_| RestartIndexError::Invalid("initial receipt overflow"))?,
            checkpoint_occurrences: checkpoint_high_water,
            exact_labels: u64::try_from(by_label.len())
                .map_err(|_| RestartIndexError::Invalid("label receipt overflow"))?,
            sequence_branches_allocated: u64::try_from(sequence_receipt.branches_allocated)
                .map_err(|_| RestartIndexError::Invalid("branch receipt overflow"))?,
            append_splice_branches_allocated: u64::try_from(append_splice_branches_allocated)
                .map_err(|_| RestartIndexError::Invalid("append receipt overflow"))?,
            selected_checkpoint_root_edges: u64::try_from(prefix_label_count)
                .map_err(|_| RestartIndexError::Invalid("checkpoint edge receipt overflow"))?,
            selected_checkpoint_history_nodes_upper_bound: checkpoint_history_nodes_upper_bound,
        },
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LabelMutationKind {
    Delete {
        expected: ReferenceOccurrence,
    },
    Insert {
        occurrence: ReferenceOccurrence,
        descriptor: ArenaId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LabelMutationPhase {
    RetainSequence,
    AllocateInsertedLeaf,
    BeginSequenceSplice,
    PollSequenceSplice,
    TakeSequence,
    AllocateDirectoryLeaf,
    ReleaseSequence,
    BeginDirectorySplice,
    PollDirectorySplice,
    Complete,
    Failed,
}

#[derive(Debug)]
struct ActiveLabelMutation {
    build: crate::ArenaBuildId,
    kind: LabelMutationKind,
    label_id: u64,
    prefix_rank: u64,
    directory_lookup: DirectoryLookup,
    working_directory: Option<ArenaBuildOwner>,
    working_sequence: Option<ArenaBuildOwner>,
    inserted_leaf: Option<ArenaBuildOwner>,
    sequence_splice: Option<ResumableSequenceSplice<LabelOccurrenceSpec>>,
    updated_sequence: Option<ArenaBuildOwner>,
    directory_leaf: Option<ArenaBuildOwner>,
    directory_splice: Option<ResumableSequenceSplice<ExactLabelDirectorySpec>>,
    phase: LabelMutationPhase,
}

impl ActiveLabelMutation {
    fn try_new(
        session: &ArenaBuildSession<'_>,
        checkpoint_directory: Option<ArenaId>,
        working_directory: Option<ArenaBuildOwner>,
        kind: LabelMutationKind,
        query_receipt: &mut QueryWorkReceipt,
    ) -> Result<Self, RestartIndexError> {
        let label_id = match kind {
            LabelMutationKind::Delete { expected } => expected.label_id,
            LabelMutationKind::Insert { occurrence, .. } => occurrence.label_id,
        };
        let prefix = lookup_directory_counted(
            session.arena(),
            checkpoint_directory,
            label_id,
            query_receipt,
        )?;
        // A suffix-only label is absent from the checkpoint directory and its
        // authenticated prefix rank is exactly zero.
        let prefix_rank = prefix.sequence_length;
        let working_root = working_directory
            .as_ref()
            .map(|owner| session.owner_id(owner))
            .transpose()?;
        let directory_lookup =
            lookup_directory_counted(session.arena(), working_root, label_id, query_receipt)?;
        if prefix_rank > directory_lookup.sequence_length {
            return Err(RestartIndexError::Invalid(
                "checkpoint prefix rank exceeds the full per-label sequence",
            ));
        }
        if let LabelMutationKind::Delete { expected } = kind {
            if prefix_rank >= directory_lookup.sequence_length {
                return Err(RestartIndexError::Invalid(
                    "old changed occurrence is absent at its checkpoint rank",
                ));
            }
            let actual = per_label_occurrence_at_counted(
                session.arena(),
                directory_lookup.sequence_root,
                prefix_rank,
                query_receipt,
            )?
            .2;
            if actual != expected {
                return Err(RestartIndexError::Invalid(
                    "old changed occurrence disagrees with the per-label restart cut",
                ));
            }
        }
        Ok(Self {
            build: session.id(),
            kind,
            label_id,
            prefix_rank,
            directory_lookup,
            working_directory,
            working_sequence: None,
            inserted_leaf: None,
            sequence_splice: None,
            updated_sequence: None,
            directory_leaf: None,
            directory_splice: None,
            phase: LabelMutationPhase::RetainSequence,
        })
    }

    fn try_new_append(
        session: &ArenaBuildSession<'_>,
        working_directory: Option<ArenaBuildOwner>,
        occurrence: ReferenceOccurrence,
        descriptor: ArenaId,
        query_receipt: &mut QueryWorkReceipt,
    ) -> Result<Self, RestartIndexError> {
        let working_root = working_directory
            .as_ref()
            .map(|owner| session.owner_id(owner))
            .transpose()?;
        let directory_lookup = lookup_directory_counted(
            session.arena(),
            working_root,
            occurrence.label_id,
            query_receipt,
        )?;
        Ok(Self {
            build: session.id(),
            kind: LabelMutationKind::Insert {
                occurrence,
                descriptor,
            },
            label_id: occurrence.label_id,
            // Streaming construction appends in source order. Existing
            // element zero therefore remains the first-definition winner.
            prefix_rank: directory_lookup.sequence_length,
            directory_lookup,
            working_directory,
            working_sequence: None,
            inserted_leaf: None,
            sequence_splice: None,
            updated_sequence: None,
            directory_leaf: None,
            directory_splice: None,
            phase: LabelMutationPhase::RetainSequence,
        })
    }

    const fn inserts_new_exact_label(&self) -> bool {
        self.directory_lookup.leaf.is_none()
    }

    fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<bool, RestartIndexError> {
        if session.id() != self.build {
            return Err(RestartIndexError::Invalid(
                "label mutation crossed arena build authority",
            ));
        }
        match self.phase {
            LabelMutationPhase::RetainSequence => {
                self.working_sequence = self
                    .directory_lookup
                    .sequence_root
                    .map(|root| session.retain(root))
                    .transpose()?;
                self.phase = match self.kind {
                    LabelMutationKind::Delete { .. } => LabelMutationPhase::BeginSequenceSplice,
                    LabelMutationKind::Insert { .. } => LabelMutationPhase::AllocateInsertedLeaf,
                };
            }
            LabelMutationPhase::AllocateInsertedLeaf => {
                let LabelMutationKind::Insert {
                    occurrence,
                    descriptor,
                } = self.kind
                else {
                    return Err(RestartIndexError::Invalid(
                        "delete mutation entered insert allocation",
                    ));
                };
                let payload = encode_label_occurrence(occurrence);
                let (leaf, _) = session.allocate_packed(&payload, &[descriptor])?;
                self.inserted_leaf = Some(leaf);
                self.phase = LabelMutationPhase::BeginSequenceSplice;
            }
            LabelMutationPhase::BeginSequenceSplice => {
                let range = match self.kind {
                    LabelMutationKind::Delete { .. } => self.prefix_rank..self.prefix_rank + 1,
                    LabelMutationKind::Insert { .. } => self.prefix_rank..self.prefix_rank,
                };
                let mut splice = ResumableSequenceSplice::<LabelOccurrenceSpec>::try_from_owned(
                    session,
                    self.working_sequence.take(),
                    range,
                    self.inserted_leaf.take(),
                    receipt,
                )?;
                // Even direct empty/singleton cases pass through the same
                // explicit poll boundary.
                if splice.poll(session, receipt)? == ResumableSequenceSplitProgress::Complete {
                    self.sequence_splice = Some(splice);
                    self.phase = LabelMutationPhase::TakeSequence;
                } else {
                    self.sequence_splice = Some(splice);
                    self.phase = LabelMutationPhase::PollSequenceSplice;
                }
            }
            LabelMutationPhase::PollSequenceSplice => {
                let splice = self
                    .sequence_splice
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "active per-label splice disappeared",
                    ))?;
                if splice.poll(session, receipt)? == ResumableSequenceSplitProgress::Complete {
                    self.phase = LabelMutationPhase::TakeSequence;
                }
            }
            LabelMutationPhase::TakeSequence => {
                self.updated_sequence = self
                    .sequence_splice
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "completed per-label splice disappeared",
                    ))?
                    .take_root()?;
                self.phase = if self.updated_sequence.is_some() {
                    LabelMutationPhase::AllocateDirectoryLeaf
                } else {
                    // Empty labels are removed, never retained as tombstones.
                    LabelMutationPhase::BeginDirectorySplice
                };
            }
            LabelMutationPhase::AllocateDirectoryLeaf => {
                let sequence = self
                    .updated_sequence
                    .as_ref()
                    .ok_or(RestartIndexError::Invalid(
                        "directory leaf lost its per-label sequence",
                    ))?;
                let sequence_id = session.owner_id(sequence)?;
                let length =
                    sequence_length::<LabelOccurrenceSpec>(session.arena(), Some(sequence_id))?;
                let payload = encode_directory_leaf(self.label_id, length);
                let (leaf, _) = session.allocate_packed(&payload, &[sequence_id])?;
                self.directory_leaf = Some(leaf);
                self.phase = LabelMutationPhase::ReleaseSequence;
            }
            LabelMutationPhase::ReleaseSequence => {
                let sequence = self
                    .updated_sequence
                    .take()
                    .ok_or(RestartIndexError::Invalid(
                        "directory child owner disappeared",
                    ))?;
                session.release(sequence)?;
                self.phase = LabelMutationPhase::BeginDirectorySplice;
            }
            LabelMutationPhase::BeginDirectorySplice => {
                let range = if self.directory_lookup.leaf.is_some() {
                    self.directory_lookup.insertion_index..self.directory_lookup.insertion_index + 1
                } else {
                    self.directory_lookup.insertion_index..self.directory_lookup.insertion_index
                };
                if matches!(self.kind, LabelMutationKind::Delete { .. })
                    && self.directory_lookup.leaf.is_none()
                {
                    return Err(RestartIndexError::Invalid(
                        "delete mutation lost its exact-label directory leaf",
                    ));
                }
                let mut splice =
                    ResumableSequenceSplice::<ExactLabelDirectorySpec>::try_from_owned(
                        session,
                        self.working_directory.take(),
                        range,
                        self.directory_leaf.take(),
                        receipt,
                    )?;
                if splice.poll(session, receipt)? == ResumableSequenceSplitProgress::Complete {
                    self.directory_splice = Some(splice);
                    self.phase = LabelMutationPhase::Complete;
                } else {
                    self.directory_splice = Some(splice);
                    self.phase = LabelMutationPhase::PollDirectorySplice;
                }
            }
            LabelMutationPhase::PollDirectorySplice => {
                let splice = self
                    .directory_splice
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "active label-directory splice disappeared",
                    ))?;
                if splice.poll(session, receipt)? == ResumableSequenceSplitProgress::Complete {
                    self.phase = LabelMutationPhase::Complete;
                }
            }
            LabelMutationPhase::Complete => return Ok(true),
            LabelMutationPhase::Failed => {
                return Err(RestartIndexError::Invalid("per-label mutation is poisoned"));
            }
        }
        Ok(self.phase == LabelMutationPhase::Complete)
    }

    fn take_directory(&mut self) -> Result<Option<ArenaBuildOwner>, RestartIndexError> {
        if self.phase != LabelMutationPhase::Complete {
            return Err(RestartIndexError::Invalid(
                "per-label mutation is not complete",
            ));
        }
        let output = self
            .directory_splice
            .as_mut()
            .ok_or(RestartIndexError::Invalid(
                "completed label-directory splice disappeared",
            ))?
            .take_root()?;
        self.phase = LabelMutationPhase::Failed;
        Ok(output)
    }
}

/// CandidateWriter-authenticated identity for one initial semantic-index
/// build. The scalar fields are deliberately not accepted by the builder's
/// public constructor; the concrete writer adapter will mint this value after
/// joining source, Green, and parser terminal authority.
#[derive(Debug)]
pub(crate) struct ReferenceCandidateIndexAuthority {
    build: crate::ArenaBuildId,
    source: SourceSnapshotDescriptor,
    source_lineage_generation: u64,
    green_generation: u64,
    authority_nonce: u64,
}

impl ReferenceCandidateIndexAuthority {
    pub(crate) fn from_writer_join(
        build: crate::ArenaBuildId,
        source: SourceSnapshotDescriptor,
        source_lineage_generation: u64,
        green_generation: u64,
        authority_nonce: u64,
        _mint: &mut crate::candidate_writer::ReferenceCandidateIndexWriterMint,
    ) -> Result<Self, RestartIndexError> {
        let authority = Self {
            build,
            source,
            source_lineage_generation,
            green_generation,
            authority_nonce,
        };
        authority.validate()?;
        Ok(authority)
    }

    #[cfg(test)]
    const fn proof_only(
        build: crate::ArenaBuildId,
        source: SourceSnapshotDescriptor,
        source_lineage_generation: u64,
        green_generation: u64,
        authority_nonce: u64,
    ) -> Self {
        Self {
            build,
            source,
            source_lineage_generation,
            green_generation,
            authority_nonce,
        }
    }

    fn validate(&self) -> Result<(), RestartIndexError> {
        if self.source.root.0 == 0
            || self.source_lineage_generation == 0
            || self.green_generation == 0
            || self.authority_nonce == 0
        {
            return Err(RestartIndexError::Invalid(
                "candidate reference-index authority is incomplete",
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
struct PendingCandidateOccurrence {
    occurrence: ReferenceOccurrence,
    label_use: Option<ReferenceLabelUseAck>,
    destination: Option<ArenaBuildOwner>,
    title: Option<ArenaBuildOwner>,
    descriptor: Option<ArenaId>,
    inserts_new_exact_label: bool,
}

#[derive(Debug)]
struct CapturedReferenceCheckpoint {
    occurrence_high_water: u64,
    prefix_directory: Option<ArenaBuildOwner>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceCandidateIndexProgress {
    ReadyForOccurrence,
    Pending,
    OccurrenceAckReady,
    ManifestReady,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReferenceCandidateIndexPhase {
    Ready,
    AllocateGlobalLeaf,
    ReleaseDestinationBlob,
    ReleaseTitleBlob,
    PushGlobalLeaf,
    StartLabelMutation,
    PollLabelMutation,
    OccurrenceAckReady,
    FinishGlobal,
    AllocateGreen,
    AllocateCheckpoint,
    ReleaseCheckpointDirectory,
    AllocateInternerAuthority,
    ReleaseInternerManifest,
    AllocateSourceLineage,
    AllocateOccurrenceAllocator,
    AllocateDocument,
    ReleaseGlobal,
    ReleaseDirectory,
    ReleaseCheckpoint,
    ReleaseInterner,
    ReleaseSourceLineage,
    ReleaseGreen,
    ReleaseOccurrenceAllocator,
    ManifestReady,
    Taken,
    Failed,
}

#[derive(Debug)]
struct PendingReferenceInternerManifest {
    owner: ArenaBuildOwner,
    identity: ReferenceLabelInternerManifestIdentity,
}

/// One unpublished initial reference-index manifest. CandidateWriter consumes
/// this owner into its composite terminal root; tests may commit it directly.
#[derive(Debug)]
#[must_use = "the candidate reference index must join the composite manifest"]
pub(crate) struct ReferenceCandidateIndexManifest {
    owner: Option<ArenaBuildOwner>,
    checkpoint: ArenaId,
    receipt: ReferenceCandidateIndexReceipt,
}

impl ReferenceCandidateIndexManifest {
    pub(crate) fn consume_for_candidate_writer(
        mut self,
        _mint: &mut crate::candidate_writer::ReferenceCandidateIndexWriterMint,
    ) -> Result<ArenaBuildOwner, RestartIndexError> {
        self.owner.take().ok_or(RestartIndexError::Invalid(
            "candidate reference-index manifest was already consumed",
        ))
    }

    pub(crate) const fn checkpoint(&self) -> ArenaId {
        self.checkpoint
    }

    pub(crate) const fn receipt(&self) -> ReferenceCandidateIndexReceipt {
        self.receipt
    }

    #[cfg(test)]
    fn commit_for_test(
        mut self,
        session: ArenaBuildSession<'_>,
    ) -> Result<RestartIndexDocument, RestartIndexError> {
        let checkpoint = session.arena().scoped_query_id(self.checkpoint)?;
        let receipt = self.receipt;
        let owner = session.commit(self.owner.take().ok_or(RestartIndexError::Invalid(
            "candidate reference-index manifest was already consumed",
        ))?)?;
        Ok(RestartIndexDocument {
            owner: Some(owner),
            checkpoint,
            initial_receipt: InitialBuildReceipt {
                occurrences: receipt.occurrences_acknowledged,
                checkpoint_occurrences: receipt.checkpoint_occurrences,
                exact_labels: receipt.exact_labels,
                sequence_branches_allocated: u64::try_from(receipt.sequence_branches_allocated)
                    .map_err(|_| RestartIndexError::Invalid("candidate branch receipt overflow"))?,
                ..InitialBuildReceipt::default()
            },
        })
    }
}

/// Resumable initial builder for the same document/directory topology consumed
/// by [`RestartIndexUpdateJob`]. It accepts exactly one linear occurrence,
/// never groups a document in memory, and withholds the label-use ack until
/// both persistent orderings include that occurrence.
#[derive(Debug)]
pub(crate) struct ReferenceCandidateIndexBuilder {
    authority: ReferenceCandidateIndexAuthority,
    interner_mint: ReferenceIndexInternerMint,
    interner_generation: Option<u64>,
    occurrence_count: u64,
    global_builder: ResumableStreamingSequenceBuilder<GlobalOccurrenceSpec>,
    global_root: Option<ArenaBuildOwner>,
    working_directory: Option<ArenaBuildOwner>,
    active_label: Option<ActiveLabelMutation>,
    pending: Option<PendingCandidateOccurrence>,
    ready_ack: Option<ReferenceCandidateOccurrenceAck>,
    checkpoint: Option<CapturedReferenceCheckpoint>,
    interner_manifest: Option<PendingReferenceInternerManifest>,
    interner_authority: Option<ArenaBuildOwner>,
    green: Option<ArenaBuildOwner>,
    checkpoint_owner: Option<ArenaBuildOwner>,
    checkpoint_id: Option<ArenaId>,
    source_lineage: Option<ArenaBuildOwner>,
    occurrence_allocator: Option<ArenaBuildOwner>,
    terminal: Option<ArenaBuildOwner>,
    phase: ReferenceCandidateIndexPhase,
    fault_after_task: Option<u64>,
    sequence_receipt: SequenceMutationReceipt,
    query_receipt: QueryWorkReceipt,
    receipt: ReferenceCandidateIndexReceipt,
}

impl ReferenceCandidateIndexBuilder {
    pub(crate) fn new(
        authority: ReferenceCandidateIndexAuthority,
    ) -> Result<Self, RestartIndexError> {
        authority.validate()?;
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let global_builder = ResumableStreamingSequenceBuilder::<GlobalOccurrenceSpec>::try_new(
            &mut sequence_receipt,
        )?;
        Ok(Self {
            authority,
            interner_mint: ReferenceIndexInternerMint::new(),
            interner_generation: None,
            occurrence_count: 0,
            global_builder,
            global_root: None,
            working_directory: None,
            active_label: None,
            pending: None,
            ready_ack: None,
            checkpoint: None,
            interner_manifest: None,
            interner_authority: None,
            green: None,
            checkpoint_owner: None,
            checkpoint_id: None,
            source_lineage: None,
            occurrence_allocator: None,
            terminal: None,
            phase: ReferenceCandidateIndexPhase::Ready,
            fault_after_task: None,
            sequence_receipt,
            query_receipt: QueryWorkReceipt::default(),
            receipt: ReferenceCandidateIndexReceipt::default(),
        })
    }

    #[cfg(test)]
    fn with_fault_after_task(mut self, task: u64) -> Self {
        self.fault_after_task = Some(task);
        self
    }

    pub(crate) const fn receipt(&self) -> ReferenceCandidateIndexReceipt {
        self.receipt
    }

    pub(crate) fn begin_occurrence(
        &mut self,
        session: &ArenaBuildSession<'_>,
        occurrence: WriterAuthenticatedReferenceOccurrence,
    ) -> Result<(), RestartIndexError> {
        if session.id() != self.authority.build
            || self.phase != ReferenceCandidateIndexPhase::Ready
            || self.pending.is_some()
            || self.ready_ack.is_some()
        {
            return Err(RestartIndexError::Invalid(
                "candidate reference index is not ready for one occurrence",
            ));
        }
        let WriterAuthenticatedReferenceOccurrence {
            label,
            destination,
            title,
        } = occurrence;
        let destination_metadata = destination.metadata(session)?;
        let title_metadata = title
            .as_ref()
            .map(|blob| blob.metadata(session))
            .transpose()?;
        let (label_id, label_use) = label.consume_for_reference_index(&mut self.interner_mint);
        let next_occurrence_id =
            self.occurrence_count
                .checked_add(1)
                .ok_or(RestartIndexError::Invalid(
                    "reference occurrence identity allocator is exhausted",
                ))?;
        let title_descriptor = title_metadata.map(|metadata| CookedReferenceBlobDescriptor {
            root: metadata.root,
            bytes: metadata.bytes,
        });
        let stored = ReferenceOccurrence {
            occurrence_id: next_occurrence_id,
            label_id,
            value: ReferenceOccurrenceValue::Cooked(CookedReferenceValueDescriptor {
                destination_root: destination_metadata.root,
                destination_bytes: destination_metadata.bytes,
                title: title_descriptor,
            }),
        };
        stored.validate()?;
        self.pending = Some(PendingCandidateOccurrence {
            occurrence: stored,
            label_use: Some(label_use),
            destination: destination.into_owner(),
            title: title.and_then(PersistentByteBlob::into_owner),
            descriptor: None,
            inserts_new_exact_label: false,
        });
        self.receipt.maximum_pending_occurrences = self.receipt.maximum_pending_occurrences.max(1);
        self.phase = ReferenceCandidateIndexPhase::AllocateGlobalLeaf;
        Ok(())
    }

    pub(crate) fn capture_checkpoint(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(), RestartIndexError> {
        if session.id() != self.authority.build
            || self.phase != ReferenceCandidateIndexPhase::Ready
            || self.checkpoint.is_some()
            || self.pending.is_some()
            || self.ready_ack.is_some()
        {
            return Err(RestartIndexError::Invalid(
                "reference checkpoint capture requires an idle builder and one selection",
            ));
        }
        let prefix_directory = self
            .working_directory
            .as_ref()
            .map(|owner| session.owner_id(owner))
            .transpose()?
            .map(|root| session.retain(root))
            .transpose()?;
        self.checkpoint = Some(CapturedReferenceCheckpoint {
            occurrence_high_water: self.occurrence_count,
            prefix_directory,
        });
        self.receipt.checkpoint_occurrences = self.occurrence_count;
        Ok(())
    }

    pub(crate) fn begin_finish(
        &mut self,
        session: &ArenaBuildSession<'_>,
        interner_manifest: ReferenceLabelInternerManifest,
    ) -> Result<(), RestartIndexError> {
        let identity = interner_manifest.identity();
        if session.id() != self.authority.build
            || self.phase != ReferenceCandidateIndexPhase::Ready
            || self.checkpoint.is_none()
            || self.pending.is_some()
            || self.ready_ack.is_some()
            || identity.build != self.authority.build
            || identity.generation == 0
            || identity.label_count != self.receipt.exact_labels
            || identity.label_id_high_water < self.receipt.exact_labels
        {
            return Err(RestartIndexError::Invalid(
                "candidate reference-index finish crossed checkpoint/interner authority",
            ));
        }
        let owner = interner_manifest.consume_for_reference_index(&mut self.interner_mint);
        session.owner_id(&owner)?;
        self.interner_generation = Some(identity.generation);
        self.interner_manifest = Some(PendingReferenceInternerManifest { owner, identity });
        if self.occurrence_count == 0 {
            self.phase = ReferenceCandidateIndexPhase::AllocateGreen;
        } else {
            self.global_builder
                .begin_finish(&mut self.sequence_receipt)?;
            self.phase = ReferenceCandidateIndexPhase::FinishGlobal;
        }
        Ok(())
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<ReferenceCandidateIndexProgress, RestartIndexError> {
        if session.id() != self.authority.build {
            return Err(RestartIndexError::Invalid(
                "candidate reference-index poll crossed arena build authority",
            ));
        }
        match self.phase {
            ReferenceCandidateIndexPhase::Ready => {
                return Ok(ReferenceCandidateIndexProgress::ReadyForOccurrence);
            }
            ReferenceCandidateIndexPhase::OccurrenceAckReady => {
                return Ok(ReferenceCandidateIndexProgress::OccurrenceAckReady);
            }
            ReferenceCandidateIndexPhase::ManifestReady => {
                return Ok(ReferenceCandidateIndexProgress::ManifestReady);
            }
            ReferenceCandidateIndexPhase::Failed | ReferenceCandidateIndexPhase::Taken => {
                return Err(RestartIndexError::Invalid(
                    "candidate reference-index builder is poisoned or consumed",
                ));
            }
            _ => {}
        }
        let branches_before = self.sequence_receipt.branches_allocated;
        let query_before = self.query_receipt.sequence_nodes_visited;
        let result = self.poll_one(session);
        let branches_this_task = self
            .sequence_receipt
            .branches_allocated
            .saturating_sub(branches_before);
        let query_this_task = self
            .query_receipt
            .sequence_nodes_visited
            .saturating_sub(query_before);
        if branches_this_task > 1 || query_this_task > MAX_QUERY_SEQUENCE_NODES_PER_TASK {
            self.phase = ReferenceCandidateIndexPhase::Failed;
            return Err(RestartIndexError::Invalid(
                "candidate reference-index task exceeded its bounded work envelope",
            ));
        }
        self.receipt.bounded_tasks =
            self.receipt
                .bounded_tasks
                .checked_add(1)
                .ok_or(RestartIndexError::Invalid(
                    "candidate reference-index task count overflow",
                ))?;
        self.receipt.sequence_branches_allocated = self.sequence_receipt.branches_allocated;
        self.receipt.maximum_branches_per_task = self
            .receipt
            .maximum_branches_per_task
            .max(branches_this_task);
        self.receipt.query_sequence_nodes_visited = self.query_receipt.sequence_nodes_visited;
        self.receipt.maximum_query_sequence_nodes_per_task = self
            .receipt
            .maximum_query_sequence_nodes_per_task
            .max(query_this_task);
        self.receipt.maximum_live_owners =
            self.receipt.maximum_live_owners.max(session.live_owners()?);
        if let Err(error) = result {
            self.phase = ReferenceCandidateIndexPhase::Failed;
            return Err(error);
        }
        if self.fault_after_task == Some(self.receipt.bounded_tasks) {
            self.phase = ReferenceCandidateIndexPhase::Failed;
            return Err(RestartIndexError::InjectedFault(self.receipt.bounded_tasks));
        }
        Ok(match self.phase {
            ReferenceCandidateIndexPhase::Ready => {
                ReferenceCandidateIndexProgress::ReadyForOccurrence
            }
            ReferenceCandidateIndexPhase::OccurrenceAckReady => {
                ReferenceCandidateIndexProgress::OccurrenceAckReady
            }
            ReferenceCandidateIndexPhase::ManifestReady => {
                ReferenceCandidateIndexProgress::ManifestReady
            }
            _ => ReferenceCandidateIndexProgress::Pending,
        })
    }

    #[allow(clippy::too_many_lines)]
    fn poll_one(&mut self, session: &mut ArenaBuildSession<'_>) -> Result<(), RestartIndexError> {
        match self.phase {
            ReferenceCandidateIndexPhase::AllocateGlobalLeaf => {
                let pending = self.pending.as_mut().ok_or(RestartIndexError::Invalid(
                    "candidate occurrence disappeared before global allocation",
                ))?;
                let leaf = allocate_global_occurrence_leaf(session, pending.occurrence)?;
                let leaf_id = session.owner_id(&leaf)?;
                pending.descriptor = Some(leaf_id);
                self.global_builder
                    .begin_push(session, leaf, &mut self.sequence_receipt)?;
                self.phase = ReferenceCandidateIndexPhase::ReleaseDestinationBlob;
            }
            ReferenceCandidateIndexPhase::ReleaseDestinationBlob => {
                if let Some(owner) = self
                    .pending
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "candidate occurrence disappeared before destination release",
                    ))?
                    .destination
                    .take()
                {
                    session.release(owner)?;
                }
                self.phase = ReferenceCandidateIndexPhase::ReleaseTitleBlob;
            }
            ReferenceCandidateIndexPhase::ReleaseTitleBlob => {
                if let Some(owner) = self
                    .pending
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "candidate occurrence disappeared before title release",
                    ))?
                    .title
                    .take()
                {
                    session.release(owner)?;
                }
                self.phase = ReferenceCandidateIndexPhase::PushGlobalLeaf;
            }
            ReferenceCandidateIndexPhase::PushGlobalLeaf => {
                if self
                    .global_builder
                    .poll_push(session, &mut self.sequence_receipt)?
                    == ResumableSequenceProgress::Complete
                {
                    self.phase = ReferenceCandidateIndexPhase::StartLabelMutation;
                }
            }
            ReferenceCandidateIndexPhase::StartLabelMutation => {
                let pending = self.pending.as_ref().ok_or(RestartIndexError::Invalid(
                    "candidate occurrence disappeared before label append",
                ))?;
                let mutation = ActiveLabelMutation::try_new_append(
                    session,
                    self.working_directory.take(),
                    pending.occurrence,
                    pending.descriptor.ok_or(RestartIndexError::Invalid(
                        "candidate occurrence lost its global descriptor",
                    ))?,
                    &mut self.query_receipt,
                )?;
                self.pending
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "candidate occurrence disappeared during label planning",
                    ))?
                    .inserts_new_exact_label = mutation.inserts_new_exact_label();
                self.active_label = Some(mutation);
                self.phase = ReferenceCandidateIndexPhase::PollLabelMutation;
            }
            ReferenceCandidateIndexPhase::PollLabelMutation => {
                let mutation = self
                    .active_label
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "candidate label mutation disappeared",
                    ))?;
                if mutation.poll(session, &mut self.sequence_receipt)? {
                    self.working_directory = mutation.take_directory()?;
                    self.active_label = None;
                    let mut pending = self.pending.take().ok_or(RestartIndexError::Invalid(
                        "completed candidate occurrence disappeared",
                    ))?;
                    self.occurrence_count =
                        self.occurrence_count
                            .checked_add(1)
                            .ok_or(RestartIndexError::Invalid(
                                "candidate occurrence count overflow",
                            ))?;
                    self.receipt.occurrences_acknowledged = self.occurrence_count;
                    if pending.inserts_new_exact_label {
                        self.receipt.exact_labels =
                            self.receipt.exact_labels.checked_add(1).ok_or(
                                RestartIndexError::Invalid("candidate exact-label count overflow"),
                            )?;
                    }
                    self.ready_ack = Some(ReferenceCandidateOccurrenceAck {
                        label_use: pending.label_use.take().ok_or(RestartIndexError::Invalid(
                            "candidate occurrence lost its label-use lineage",
                        ))?,
                    });
                    self.phase = ReferenceCandidateIndexPhase::OccurrenceAckReady;
                }
            }
            ReferenceCandidateIndexPhase::FinishGlobal => {
                if self
                    .global_builder
                    .poll_finish(session, &mut self.sequence_receipt)?
                    == ResumableSequenceProgress::Complete
                {
                    self.global_root = Some(self.global_builder.take_root()?);
                    self.phase = ReferenceCandidateIndexPhase::AllocateGreen;
                }
            }
            ReferenceCandidateIndexPhase::AllocateGreen => {
                let (green, _) = session.allocate_packed(
                    &encode_green(
                        self.authority.source.revision.0,
                        self.authority.green_generation,
                        self.authority.authority_nonce,
                    ),
                    &[],
                )?;
                self.green = Some(green);
                self.phase = ReferenceCandidateIndexPhase::AllocateCheckpoint;
            }
            ReferenceCandidateIndexPhase::AllocateCheckpoint => {
                let checkpoint = self.checkpoint.as_ref().ok_or(RestartIndexError::Invalid(
                    "candidate checkpoint disappeared",
                ))?;
                let green_id = session.owner_id(self.green.as_ref().ok_or(
                    RestartIndexError::Invalid("candidate Green authority disappeared"),
                )?)?;
                let prefix_id = checkpoint
                    .prefix_directory
                    .as_ref()
                    .map(|owner| session.owner_id(owner))
                    .transpose()?;
                let payload = encode_checkpoint_manifest(
                    self.authority.source.revision.0,
                    checkpoint.occurrence_high_water,
                    self.interner_generation.ok_or(RestartIndexError::Invalid(
                        "candidate interner generation disappeared",
                    ))?,
                    self.authority.source_lineage_generation,
                    prefix_id.is_some(),
                );
                let (owner, _) = if let Some(prefix) = prefix_id {
                    session.allocate_packed(&payload, &[prefix, green_id])?
                } else {
                    session.allocate_packed(&payload, &[green_id])?
                };
                self.checkpoint_id = Some(session.owner_id(&owner)?);
                self.checkpoint_owner = Some(owner);
                self.phase = ReferenceCandidateIndexPhase::ReleaseCheckpointDirectory;
            }
            ReferenceCandidateIndexPhase::ReleaseCheckpointDirectory => {
                if let Some(owner) = self
                    .checkpoint
                    .as_mut()
                    .and_then(|checkpoint| checkpoint.prefix_directory.take())
                {
                    session.release(owner)?;
                }
                self.phase = ReferenceCandidateIndexPhase::AllocateInternerAuthority;
            }
            ReferenceCandidateIndexPhase::AllocateInternerAuthority => {
                let interner =
                    self.interner_manifest
                        .as_ref()
                        .ok_or(RestartIndexError::Invalid(
                            "candidate interner manifest disappeared",
                        ))?;
                let interner_id = session.owner_id(&interner.owner)?;
                let (authority, _) = session.allocate_packed(
                    &encode_interner_authority(
                        interner.identity.generation,
                        interner.identity.label_id_high_water,
                        true,
                    ),
                    &[interner_id],
                )?;
                self.interner_authority = Some(authority);
                self.phase = ReferenceCandidateIndexPhase::ReleaseInternerManifest;
            }
            ReferenceCandidateIndexPhase::ReleaseInternerManifest => {
                let owner = self
                    .interner_manifest
                    .take()
                    .ok_or(RestartIndexError::Invalid(
                        "candidate interner manifest disappeared before release",
                    ))?
                    .owner;
                session.release(owner)?;
                self.phase = ReferenceCandidateIndexPhase::AllocateSourceLineage;
            }
            ReferenceCandidateIndexPhase::AllocateSourceLineage => {
                let (lineage, _) = session.allocate_packed(
                    &encode_source_lineage_authority(
                        self.authority.source_lineage_generation,
                        self.authority.source,
                    )?,
                    &[],
                )?;
                self.source_lineage = Some(lineage);
                self.phase = ReferenceCandidateIndexPhase::AllocateOccurrenceAllocator;
            }
            ReferenceCandidateIndexPhase::AllocateOccurrenceAllocator => {
                let (allocator, _) = session
                    .allocate_packed(&encode_occurrence_allocator(1, self.occurrence_count), &[])?;
                self.occurrence_allocator = Some(allocator);
                self.phase = ReferenceCandidateIndexPhase::AllocateDocument;
            }
            ReferenceCandidateIndexPhase::AllocateDocument => {
                let global_id = self
                    .global_root
                    .as_ref()
                    .map(|owner| session.owner_id(owner))
                    .transpose()?;
                let directory_id = self
                    .working_directory
                    .as_ref()
                    .map(|owner| session.owner_id(owner))
                    .transpose()?;
                let checkpoint_id = self.checkpoint_id.ok_or(RestartIndexError::Invalid(
                    "candidate checkpoint identity disappeared",
                ))?;
                let interner_id = session.owner_id(self.interner_authority.as_ref().ok_or(
                    RestartIndexError::Invalid("candidate interner authority disappeared"),
                )?)?;
                let lineage_id = session.owner_id(self.source_lineage.as_ref().ok_or(
                    RestartIndexError::Invalid("candidate source lineage disappeared"),
                )?)?;
                let green_id = session.owner_id(self.green.as_ref().ok_or(
                    RestartIndexError::Invalid("candidate Green authority disappeared"),
                )?)?;
                let allocator_id = session.owner_id(self.occurrence_allocator.as_ref().ok_or(
                    RestartIndexError::Invalid("candidate occurrence allocator disappeared"),
                )?)?;
                let mut children = Vec::with_capacity(DOCUMENT_BASE_CHILDREN);
                children.extend(global_id);
                children.extend(directory_id);
                children.extend([
                    checkpoint_id,
                    interner_id,
                    lineage_id,
                    green_id,
                    allocator_id,
                ]);
                let checkpoint_high_water = self
                    .checkpoint
                    .as_ref()
                    .ok_or(RestartIndexError::Invalid(
                        "candidate checkpoint disappeared before terminal allocation",
                    ))?
                    .occurrence_high_water;
                let payload = encode_document_manifest(
                    self.authority.source.revision.0,
                    self.authority.source.revision.0,
                    self.occurrence_count,
                    checkpoint_high_water,
                    self.occurrence_count,
                    0,
                    self.interner_generation.ok_or(RestartIndexError::Invalid(
                        "candidate interner generation disappeared",
                    ))?,
                    self.authority.source_lineage_generation,
                    1,
                    self.occurrence_count,
                    Some(self.authority.source),
                    global_id.is_some(),
                    directory_id.is_some(),
                    false,
                )?;
                let (terminal, _) = session.allocate_packed(&payload, &children)?;
                self.receipt.manifest_children_joined = children.len();
                self.terminal = Some(terminal);
                self.phase = ReferenceCandidateIndexPhase::ReleaseGlobal;
            }
            ReferenceCandidateIndexPhase::ReleaseGlobal => {
                if let Some(owner) = self.global_root.take() {
                    session.release(owner)?;
                }
                self.phase = ReferenceCandidateIndexPhase::ReleaseDirectory;
            }
            ReferenceCandidateIndexPhase::ReleaseDirectory => {
                if let Some(owner) = self.working_directory.take() {
                    session.release(owner)?;
                }
                self.phase = ReferenceCandidateIndexPhase::ReleaseCheckpoint;
            }
            ReferenceCandidateIndexPhase::ReleaseCheckpoint => {
                session.release(self.checkpoint_owner.take().ok_or(
                    RestartIndexError::Invalid("candidate checkpoint owner disappeared"),
                )?)?;
                self.phase = ReferenceCandidateIndexPhase::ReleaseInterner;
            }
            ReferenceCandidateIndexPhase::ReleaseInterner => {
                session.release(self.interner_authority.take().ok_or(
                    RestartIndexError::Invalid("candidate interner owner disappeared"),
                )?)?;
                self.phase = ReferenceCandidateIndexPhase::ReleaseSourceLineage;
            }
            ReferenceCandidateIndexPhase::ReleaseSourceLineage => {
                session.release(self.source_lineage.take().ok_or(
                    RestartIndexError::Invalid("candidate lineage owner disappeared"),
                )?)?;
                self.phase = ReferenceCandidateIndexPhase::ReleaseGreen;
            }
            ReferenceCandidateIndexPhase::ReleaseGreen => {
                session.release(self.green.take().ok_or(RestartIndexError::Invalid(
                    "candidate Green owner disappeared",
                ))?)?;
                self.phase = ReferenceCandidateIndexPhase::ReleaseOccurrenceAllocator;
            }
            ReferenceCandidateIndexPhase::ReleaseOccurrenceAllocator => {
                session.release(self.occurrence_allocator.take().ok_or(
                    RestartIndexError::Invalid("candidate allocator owner disappeared"),
                )?)?;
                // This builder is one child in CandidateWriter's shared
                // journal, where the completed Green child may already be
                // live. Validate only the owner this child controls; the
                // terminal composite join is the sole layer that checks the
                // complete journal's exact two-children-to-one-parent shape.
                session.owner_id(self.terminal.as_ref().ok_or(RestartIndexError::Invalid(
                    "candidate reference-index terminal owner disappeared",
                ))?)?;
                self.phase = ReferenceCandidateIndexPhase::ManifestReady;
            }
            ReferenceCandidateIndexPhase::Ready
            | ReferenceCandidateIndexPhase::OccurrenceAckReady
            | ReferenceCandidateIndexPhase::ManifestReady
            | ReferenceCandidateIndexPhase::Taken
            | ReferenceCandidateIndexPhase::Failed => {
                return Err(RestartIndexError::Invalid(
                    "candidate reference-index phase is not pollable",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn take_occurrence_ack(
        &mut self,
    ) -> Result<ReferenceCandidateOccurrenceAck, RestartIndexError> {
        if self.phase != ReferenceCandidateIndexPhase::OccurrenceAckReady {
            return Err(RestartIndexError::Invalid(
                "candidate occurrence acknowledgement is not ready",
            ));
        }
        let ack = self.ready_ack.take().ok_or(RestartIndexError::Invalid(
            "candidate occurrence acknowledgement disappeared",
        ))?;
        self.phase = ReferenceCandidateIndexPhase::Ready;
        Ok(ack)
    }

    pub(crate) fn take_manifest(
        &mut self,
    ) -> Result<ReferenceCandidateIndexManifest, RestartIndexError> {
        if self.phase != ReferenceCandidateIndexPhase::ManifestReady {
            return Err(RestartIndexError::Invalid(
                "candidate reference-index manifest is not ready",
            ));
        }
        let manifest = ReferenceCandidateIndexManifest {
            owner: Some(self.terminal.take().ok_or(RestartIndexError::Invalid(
                "candidate reference-index terminal owner disappeared",
            ))?),
            checkpoint: self.checkpoint_id.ok_or(RestartIndexError::Invalid(
                "candidate reference-index checkpoint identity disappeared",
            ))?,
            receipt: self.receipt,
        };
        self.phase = ReferenceCandidateIndexPhase::Taken;
        Ok(manifest)
    }
}

/// Parser-finalizer capability for one production restart. It binds the new
/// source snapshot and convergence high-water to one exact committed semantic
/// root/checkpoint; the restart builder accepts no caller-authored old range.
#[derive(Debug)]
pub(crate) struct AuthenticatedReferenceRestartAuthority {
    build: crate::ArenaBuildId,
    donor: CommittedReferenceIndex,
    donor_manifest: DocumentManifest,
    checkpoint: CheckpointManifest,
    old_join_high_water: u64,
    candidate_source: SourceSnapshotDescriptor,
    source_lineage_generation: u64,
    green_generation: u64,
    authority_nonce: u64,
}

impl AuthenticatedReferenceRestartAuthority {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_writer_join(
        arena: &PageArena,
        build: crate::ArenaBuildId,
        donor: CommittedReferenceIndex,
        edit_occurrence_start: u64,
        old_join_high_water: u64,
        candidate_source: SourceSnapshotDescriptor,
        source_lineage_generation: u64,
        green_generation: u64,
        authority_nonce: u64,
        _mint: &mut crate::candidate_writer::ReferenceCandidateIndexWriterMint,
    ) -> Result<Self, RestartIndexError> {
        Self::validate_and_new(
            arena,
            build,
            donor,
            edit_occurrence_start,
            old_join_high_water,
            candidate_source,
            source_lineage_generation,
            green_generation,
            authority_nonce,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(test)]
    fn proof_only(
        arena: &PageArena,
        build: crate::ArenaBuildId,
        donor: CommittedReferenceIndex,
        edit_occurrence_start: u64,
        old_join_high_water: u64,
        candidate_source: SourceSnapshotDescriptor,
        source_lineage_generation: u64,
        green_generation: u64,
        authority_nonce: u64,
    ) -> Result<Self, RestartIndexError> {
        Self::validate_and_new(
            arena,
            build,
            donor,
            edit_occurrence_start,
            old_join_high_water,
            candidate_source,
            source_lineage_generation,
            green_generation,
            authority_nonce,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_and_new(
        arena: &PageArena,
        build: crate::ArenaBuildId,
        donor: CommittedReferenceIndex,
        edit_occurrence_start: u64,
        old_join_high_water: u64,
        candidate_source: SourceSnapshotDescriptor,
        source_lineage_generation: u64,
        green_generation: u64,
        authority_nonce: u64,
    ) -> Result<Self, RestartIndexError> {
        let donor_manifest = donor.manifest(arena)?;
        let checkpoint_id = arena.local_id(donor.checkpoint)?;
        let checkpoint = decode_checkpoint_manifest(arena, checkpoint_id)?;
        if checkpoint_id != donor_manifest.checkpoint_root
            || checkpoint.source_revision != donor_manifest.source_revision
            || checkpoint.occurrence_high_water != donor_manifest.restart_high_water
            || checkpoint.interner_generation != donor_manifest.interner_generation
            || checkpoint.source_lineage_generation != donor_manifest.source_lineage_generation
            || edit_occurrence_start < checkpoint.occurrence_high_water
            || edit_occurrence_start > old_join_high_water
            || old_join_high_water > donor_manifest.occurrence_count
            || candidate_source.revision.0 <= donor_manifest.source_revision
            || candidate_source.root.0 == 0
            || source_lineage_generation <= donor_manifest.source_lineage_generation
            || green_generation == 0
            || authority_nonce == 0
        {
            return Err(RestartIndexError::Invalid(
                "production reference restart authority is incomplete or crossed",
            ));
        }
        for root in [
            donor_manifest.global_root,
            donor_manifest.directory_root,
            checkpoint.prefix_directory,
        ]
        .into_iter()
        .flatten()
        {
            let height = if Some(root) == donor_manifest.global_root {
                GlobalOccurrenceSpec::height(sequence_node::<GlobalOccurrenceSpec>(arena, root)?.0)
            } else {
                ExactLabelDirectorySpec::height(
                    sequence_node::<ExactLabelDirectorySpec>(arena, root)?.0,
                )
            };
            if height > MAX_AUTHENTICATED_SEQUENCE_HEIGHT {
                return Err(RestartIndexError::Invalid(
                    "restart authority exceeds the authenticated sequence-height envelope",
                ));
            }
        }
        Ok(Self {
            build,
            donor,
            donor_manifest,
            checkpoint,
            old_join_high_water,
            candidate_source,
            source_lineage_generation,
            green_generation,
            authority_nonce,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReferenceRestartIndexReceipt {
    bounded_tasks: u64,
    occurrences_spooled: u64,
    old_changed_occurrences_read: u64,
    replacement_occurrences_reverse_read: u64,
    suffix_occurrences_enumerated: u64,
    maximum_pending_occurrences: usize,
    document_sized_occurrence_vectors: usize,
    replacement_spool_roots: usize,
    interner_adoption_proven: bool,
    committed_exact_label_lookup_proven: bool,
    changed_interval_streaming_proven: bool,
    reverse_cursor_pages_read: u64,
    maximum_reverse_cursor_pages_per_task: u64,
    sequence_branches_allocated: usize,
    maximum_branches_per_task: usize,
    query_sequence_nodes_visited: u64,
    maximum_query_sequence_nodes_per_task: u64,
    maximum_live_owners: usize,
    manifest_children_joined: usize,
}

const MAX_REFERENCE_RECLAIM_FUEL_PER_TICK: usize = 1_024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReferenceIndexReclaimServiceReceipt {
    ticks: u64,
    reference_transitions: u64,
    nodes_reclaimed: u64,
    maximum_pending_before_tick: usize,
    maximum_pending_after_tick: usize,
    maximum_transitions_per_tick: usize,
}

/// Explicit scheduler-owned maintenance lane for persistent reference data.
///
/// Builders yield their arena lease before this service runs. One tick spends
/// at most its admitted reference-transition fuel; skipping ticks delays
/// reclamation but cannot mutate or invalidate the suspended candidate.
#[derive(Debug)]
pub(crate) struct ReferenceIndexReclaimService {
    fuel_per_tick: usize,
    receipt: ReferenceIndexReclaimServiceReceipt,
}

impl ReferenceIndexReclaimService {
    pub(crate) fn try_new(fuel_per_tick: usize) -> Result<Self, RestartIndexError> {
        if !(1..=MAX_REFERENCE_RECLAIM_FUEL_PER_TICK).contains(&fuel_per_tick) {
            return Err(RestartIndexError::Invalid(
                "reference reclaim fuel is zero or exceeds the scheduler envelope",
            ));
        }
        Ok(Self {
            fuel_per_tick,
            receipt: ReferenceIndexReclaimServiceReceipt::default(),
        })
    }

    pub(crate) const fn receipt(&self) -> ReferenceIndexReclaimServiceReceipt {
        self.receipt
    }

    pub(crate) fn poll(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<crate::arena::ReclaimReceipt, RestartIndexError> {
        let pending_before = arena.metrics().pending_releases;
        let receipt = arena
            .poll_reclaim(self.fuel_per_tick)
            .map_err(|failure| RestartIndexError::Arena(failure.error))?;
        if receipt.reference_transitions > self.fuel_per_tick {
            return Err(RestartIndexError::Invalid(
                "reference reclaim tick exceeded its admitted fuel",
            ));
        }
        self.receipt.ticks = self
            .receipt
            .ticks
            .checked_add(1)
            .ok_or(RestartIndexError::Invalid("reclaim tick receipt overflow"))?;
        self.receipt.reference_transitions =
            self.receipt
                .reference_transitions
                .checked_add(u64::try_from(receipt.reference_transitions).map_err(|_| {
                    RestartIndexError::Invalid("reclaim transition receipt overflow")
                })?)
                .ok_or(RestartIndexError::Invalid(
                    "reclaim transition receipt overflow",
                ))?;
        self.receipt.nodes_reclaimed = self
            .receipt
            .nodes_reclaimed
            .checked_add(
                u64::try_from(receipt.nodes_reclaimed)
                    .map_err(|_| RestartIndexError::Invalid("reclaimed-node receipt overflow"))?,
            )
            .ok_or(RestartIndexError::Invalid(
                "reclaimed-node receipt overflow",
            ))?;
        self.receipt.maximum_pending_before_tick =
            self.receipt.maximum_pending_before_tick.max(pending_before);
        self.receipt.maximum_pending_after_tick = self
            .receipt
            .maximum_pending_after_tick
            .max(receipt.pending_after);
        self.receipt.maximum_transitions_per_tick = self
            .receipt
            .maximum_transitions_per_tick
            .max(receipt.reference_transitions);
        Ok(receipt)
    }
}

impl ReferenceRestartIndexReceipt {
    pub(crate) const fn bounded_tasks(self) -> u64 {
        self.bounded_tasks
    }

    pub(crate) const fn occurrences_spooled(self) -> u64 {
        self.occurrences_spooled
    }

    pub(crate) const fn suffix_occurrences_enumerated(self) -> u64 {
        self.suffix_occurrences_enumerated
    }

    pub(crate) const fn maximum_pending_occurrences(self) -> usize {
        self.maximum_pending_occurrences
    }

    pub(crate) const fn document_sized_occurrence_vectors(self) -> usize {
        self.document_sized_occurrence_vectors
    }

    pub(crate) const fn replacement_spool_roots(self) -> usize {
        self.replacement_spool_roots
    }

    pub(crate) const fn interner_adoption_proven(self) -> bool {
        self.interner_adoption_proven
    }

    pub(crate) const fn committed_exact_label_lookup_proven(self) -> bool {
        self.committed_exact_label_lookup_proven
    }

    pub(crate) const fn changed_interval_streaming_proven(self) -> bool {
        self.changed_interval_streaming_proven
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceRestartIndexProgress {
    ReadyForOccurrence,
    Pending,
    OccurrenceAckReady,
    ManifestReady,
}

#[derive(Debug)]
struct PendingRestartOccurrence {
    occurrence: ReferenceOccurrence,
    label_use: Option<ReferenceLabelUseAck>,
    destination: Option<ArenaBuildOwner>,
    title: Option<ArenaBuildOwner>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReverseSpoolProgress {
    Pending,
    Item {
        descriptor: ArenaId,
        occurrence: ReferenceOccurrence,
    },
    Complete,
}

/// One-arena-page-per-poll reverse reader over the exact replacement global
/// sequence. This is the fixed-rank insertion source; there is no second
/// spool and no changed-interval heap collection.
#[derive(Debug)]
struct PersistentReplacementReverseCursor {
    next: Option<ArenaId>,
    pending_left: Vec<ArenaId>,
    maximum_pending_left: usize,
    expected_leaves: u64,
    observed_leaves: u64,
    pages_read: u64,
}

impl PersistentReplacementReverseCursor {
    fn try_new(
        arena: &PageArena,
        root: Option<ArenaId>,
        expected_leaves: u64,
    ) -> Result<Self, RestartIndexError> {
        let height = match root {
            Some(root) => {
                let summary = sequence_node::<GlobalOccurrenceSpec>(arena, root)?.0;
                if summary.leaves != expected_leaves
                    || summary.height > MAX_AUTHENTICATED_SEQUENCE_HEIGHT
                {
                    return Err(RestartIndexError::Invalid(
                        "replacement spool root crossed its authenticated extent",
                    ));
                }
                summary.height
            }
            None if expected_leaves == 0 => 0,
            None => {
                return Err(RestartIndexError::Invalid(
                    "nonempty replacement spool omitted its persistent root",
                ));
            }
        };
        let maximum_pending_left = usize::from(height.saturating_sub(1));
        let mut pending_left = Vec::new();
        pending_left
            .try_reserve_exact(maximum_pending_left)
            .map_err(|_| RestartIndexError::Invalid("reverse spool cursor reservation failed"))?;
        Ok(Self {
            next: root,
            pending_left,
            maximum_pending_left,
            expected_leaves,
            observed_leaves: 0,
            pages_read: 0,
        })
    }

    fn poll(&mut self, arena: &PageArena) -> Result<ReverseSpoolProgress, RestartIndexError> {
        let Some(node) = self.next.take() else {
            if !self.pending_left.is_empty() || self.observed_leaves != self.expected_leaves {
                return Err(RestartIndexError::Invalid(
                    "reverse replacement traversal ended at a crossed extent",
                ));
            }
            return Ok(ReverseSpoolProgress::Complete);
        };
        let (_, kind) = sequence_node::<GlobalOccurrenceSpec>(arena, node)?;
        self.pages_read = self
            .pages_read
            .checked_add(1)
            .ok_or(RestartIndexError::Invalid(
                "reverse spool page receipt overflow",
            ))?;
        match kind {
            SequenceNodeKind::Branch { left, right } => {
                if self.pending_left.len() >= self.maximum_pending_left {
                    return Err(RestartIndexError::Invalid(
                        "reverse spool traversal exceeded its authenticated height",
                    ));
                }
                self.pending_left.push(left);
                self.next = Some(right);
                Ok(ReverseSpoolProgress::Pending)
            }
            SequenceNodeKind::Leaf => {
                self.observed_leaves =
                    self.observed_leaves
                        .checked_add(1)
                        .ok_or(RestartIndexError::Invalid(
                            "reverse spool occurrence receipt overflow",
                        ))?;
                if self.observed_leaves > self.expected_leaves {
                    return Err(RestartIndexError::Invalid(
                        "reverse spool traversal exceeded its authenticated extent",
                    ));
                }
                self.next = self.pending_left.pop();
                Ok(ReverseSpoolProgress::Item {
                    descriptor: node,
                    occurrence: decode_global_occurrence(arena, node)?,
                })
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReferenceRestartIndexPhase {
    Ready,
    AllocateSpoolLeaf,
    ReleaseDestinationBlob,
    ReleaseTitleBlob,
    PushSpoolLeaf,
    OccurrenceAckReady,
    FinishSpool,
    AdoptSpoolRoot,
    RetainDonorGlobal,
    PollGlobalSplice,
    TakeGlobalRoot,
    RetainDonorDirectory,
    StartOldDeletion,
    PollLabelMutation,
    PollReplacementReverse,
    AllocateGreen,
    AllocateCheckpoint,
    AllocateInternerAuthority,
    ReleaseInternerManifest,
    AllocateSourceLineage,
    AllocateOccurrenceAllocator,
    AllocateDonorRange,
    AllocateSuffixAdoption,
    AllocateDocument,
    ReleaseGlobal,
    ReleaseDirectory,
    ReleaseSpoolReplay,
    ReleaseCheckpoint,
    ReleaseInterner,
    ReleaseSourceLineage,
    ReleaseGreen,
    ReleaseOccurrenceAllocator,
    ReleaseDonorRange,
    ReleaseAdoption,
    ManifestReady,
    Taken,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveRestartLabelMutationKind {
    Delete,
    Insert,
}

#[derive(Debug)]
#[must_use = "the restarted reference index must join the composite candidate"]
pub(crate) struct ReferenceRestartIndexManifest {
    owner: Option<ArenaBuildOwner>,
    checkpoint: ArenaId,
    receipt: ReferenceRestartIndexReceipt,
}

impl ReferenceRestartIndexManifest {
    pub(crate) fn consume_for_candidate_writer(
        mut self,
        _mint: &mut crate::candidate_writer::ReferenceCandidateIndexWriterMint,
    ) -> Result<ArenaBuildOwner, RestartIndexError> {
        self.owner.take().ok_or(RestartIndexError::Invalid(
            "restarted reference-index manifest was already consumed",
        ))
    }

    pub(crate) const fn checkpoint(&self) -> ArenaId {
        self.checkpoint
    }

    pub(crate) const fn receipt(&self) -> ReferenceRestartIndexReceipt {
        self.receipt
    }

    #[cfg(test)]
    fn commit_for_test(
        mut self,
        session: ArenaBuildSession<'_>,
    ) -> Result<RestartIndexDocument, RestartIndexError> {
        let checkpoint = session.arena().scoped_query_id(self.checkpoint)?;
        let owner = session.commit(self.owner.take().ok_or(RestartIndexError::Invalid(
            "restarted reference-index manifest was already consumed",
        ))?)?;
        Ok(RestartIndexDocument {
            owner: Some(owner),
            checkpoint,
            initial_receipt: InitialBuildReceipt::default(),
        })
    }
}

/// Production-shaped reference restart actor. New occurrences stream directly
/// into the persistent global replacement sequence. At convergence that one
/// tree is retained once for a bounded reverse traversal and consumed once by
/// the global splice; no changed-interval `Vec`, numeric-label input, or
/// second spool exists.
#[derive(Debug)]
pub(crate) struct ReferenceRestartIndexBuilder {
    authority: AuthenticatedReferenceRestartAuthority,
    interner_mint: ReferenceIndexInternerMint,
    adopted_interner_generation: u64,
    donor_interner_label_count: u64,
    donor_interner_id_high_water: u64,
    next_occurrence_id: u64,
    replacement_count: u64,
    spool_builder: ResumableStreamingSequenceBuilder<GlobalOccurrenceSpec>,
    pending: Option<PendingRestartOccurrence>,
    ready_ack: Option<ReferenceCandidateOccurrenceAck>,
    interner_manifest: Option<PendingReferenceInternerManifest>,
    spool_splice_owner: Option<ArenaBuildOwner>,
    spool_replay_owner: Option<ArenaBuildOwner>,
    reverse_cursor: Option<PersistentReplacementReverseCursor>,
    global_splice: Option<ResumableSequenceSplice<GlobalOccurrenceSpec>>,
    global_root: Option<ArenaBuildOwner>,
    working_directory: Option<ArenaBuildOwner>,
    active_label: Option<ActiveLabelMutation>,
    active_label_kind: Option<ActiveRestartLabelMutationKind>,
    old_cursor: u64,
    green: Option<ArenaBuildOwner>,
    checkpoint_owner: Option<ArenaBuildOwner>,
    checkpoint_id: Option<ArenaId>,
    interner_authority: Option<ArenaBuildOwner>,
    source_lineage: Option<ArenaBuildOwner>,
    occurrence_allocator: Option<ArenaBuildOwner>,
    donor_range: Option<ArenaBuildOwner>,
    adoption: Option<ArenaBuildOwner>,
    terminal: Option<ArenaBuildOwner>,
    phase: ReferenceRestartIndexPhase,
    fault_after_task: Option<u64>,
    sequence_receipt: SequenceMutationReceipt,
    query_receipt: QueryWorkReceipt,
    receipt: ReferenceRestartIndexReceipt,
}

impl ReferenceRestartIndexBuilder {
    pub(crate) fn new(
        session: &ArenaBuildSession<'_>,
        authority: AuthenticatedReferenceRestartAuthority,
    ) -> Result<(Self, ReferenceLabelInternerAdoption), RestartIndexError> {
        if session.id() != authority.build {
            return Err(RestartIndexError::Invalid(
                "restart builder crossed its arena build authority",
            ));
        }
        let committed_interner = authority.donor.interner(session.arena())?;
        if committed_interner.generation() != authority.donor_manifest.interner_generation {
            return Err(RestartIndexError::Invalid(
                "restart builder crossed its donor interner generation",
            ));
        }
        let adopted_interner_generation =
            committed_interner
                .generation()
                .checked_add(1)
                .ok_or(RestartIndexError::Invalid(
                    "restart interner generation overflow",
                ))?;
        let next_occurrence_id = authority
            .donor_manifest
            .occurrence_allocator_high_water
            .checked_add(1)
            .ok_or(RestartIndexError::Invalid(
                "restart occurrence allocator is exhausted",
            ))?;
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let spool_builder = ResumableStreamingSequenceBuilder::<GlobalOccurrenceSpec>::try_new(
            &mut sequence_receipt,
        )?;
        let receipt = ReferenceRestartIndexReceipt {
            replacement_spool_roots: 1,
            interner_adoption_proven: true,
            committed_exact_label_lookup_proven: true,
            changed_interval_streaming_proven: true,
            ..ReferenceRestartIndexReceipt::default()
        };
        let adoption = ReferenceLabelInternerAdoption::from_committed(committed_interner);
        Ok((
            Self {
                authority,
                interner_mint: ReferenceIndexInternerMint::new(),
                adopted_interner_generation,
                donor_interner_label_count: committed_interner.label_count(),
                donor_interner_id_high_water: committed_interner.label_id_high_water(),
                next_occurrence_id,
                replacement_count: 0,
                spool_builder,
                pending: None,
                ready_ack: None,
                interner_manifest: None,
                spool_splice_owner: None,
                spool_replay_owner: None,
                reverse_cursor: None,
                global_splice: None,
                global_root: None,
                working_directory: None,
                active_label: None,
                active_label_kind: None,
                old_cursor: 0,
                green: None,
                checkpoint_owner: None,
                checkpoint_id: None,
                interner_authority: None,
                source_lineage: None,
                occurrence_allocator: None,
                donor_range: None,
                adoption: None,
                terminal: None,
                phase: ReferenceRestartIndexPhase::Ready,
                fault_after_task: None,
                sequence_receipt,
                query_receipt: QueryWorkReceipt::default(),
                receipt,
            },
            adoption,
        ))
    }

    #[cfg(test)]
    fn with_fault_after_task(mut self, task: u64) -> Self {
        self.fault_after_task = Some(task);
        self
    }

    pub(crate) const fn receipt(&self) -> ReferenceRestartIndexReceipt {
        self.receipt
    }

    pub(crate) fn begin_occurrence(
        &mut self,
        session: &ArenaBuildSession<'_>,
        occurrence: WriterAuthenticatedReferenceOccurrence,
    ) -> Result<(), RestartIndexError> {
        if session.id() != self.authority.build
            || self.phase != ReferenceRestartIndexPhase::Ready
            || self.pending.is_some()
            || self.ready_ack.is_some()
        {
            return Err(RestartIndexError::Invalid(
                "restart index is not ready for one streamed occurrence",
            ));
        }
        let WriterAuthenticatedReferenceOccurrence {
            label,
            destination,
            title,
        } = occurrence;
        let destination_metadata = destination.metadata(session)?;
        let title_metadata = title
            .as_ref()
            .map(|blob| blob.metadata(session))
            .transpose()?;
        let (label_id, label_use) = label.consume_for_reference_index(&mut self.interner_mint);
        let occurrence_id = self.next_occurrence_id;
        self.next_occurrence_id =
            self.next_occurrence_id
                .checked_add(1)
                .ok_or(RestartIndexError::Invalid(
                    "restart occurrence allocator is exhausted",
                ))?;
        let occurrence = ReferenceOccurrence {
            occurrence_id,
            label_id,
            value: ReferenceOccurrenceValue::Cooked(CookedReferenceValueDescriptor {
                destination_root: destination_metadata.root,
                destination_bytes: destination_metadata.bytes,
                title: title_metadata.map(|metadata| CookedReferenceBlobDescriptor {
                    root: metadata.root,
                    bytes: metadata.bytes,
                }),
            }),
        };
        occurrence.validate()?;
        self.pending = Some(PendingRestartOccurrence {
            occurrence,
            label_use: Some(label_use),
            destination: destination.into_owner(),
            title: title.and_then(PersistentByteBlob::into_owner),
        });
        self.receipt.maximum_pending_occurrences = self.receipt.maximum_pending_occurrences.max(1);
        self.phase = ReferenceRestartIndexPhase::AllocateSpoolLeaf;
        Ok(())
    }

    pub(crate) fn take_occurrence_ack(
        &mut self,
    ) -> Result<ReferenceCandidateOccurrenceAck, RestartIndexError> {
        if self.phase != ReferenceRestartIndexPhase::OccurrenceAckReady {
            return Err(RestartIndexError::Invalid(
                "restart occurrence acknowledgement is not ready",
            ));
        }
        let ack = self.ready_ack.take().ok_or(RestartIndexError::Invalid(
            "restart occurrence acknowledgement disappeared",
        ))?;
        self.phase = ReferenceRestartIndexPhase::Ready;
        Ok(ack)
    }

    pub(crate) fn begin_finish(
        &mut self,
        session: &ArenaBuildSession<'_>,
        interner_manifest: ReferenceLabelInternerManifest,
    ) -> Result<(), RestartIndexError> {
        let identity = interner_manifest.identity();
        if session.id() != self.authority.build
            || self.phase != ReferenceRestartIndexPhase::Ready
            || self.pending.is_some()
            || self.ready_ack.is_some()
            || identity.build != self.authority.build
            || identity.generation != self.adopted_interner_generation
            || identity.label_count < self.donor_interner_label_count
            || identity.label_id_high_water < self.donor_interner_id_high_water
        {
            return Err(RestartIndexError::Invalid(
                "restart finish crossed its adopted interner authority",
            ));
        }
        let owner = interner_manifest.consume_for_reference_index(&mut self.interner_mint);
        session.owner_id(&owner)?;
        self.interner_manifest = Some(PendingReferenceInternerManifest { owner, identity });
        self.old_cursor = self.authority.checkpoint.occurrence_high_water;
        if self.replacement_count == 0 {
            self.phase = ReferenceRestartIndexPhase::RetainDonorGlobal;
        } else {
            self.spool_builder
                .begin_finish(&mut self.sequence_receipt)?;
            self.phase = ReferenceRestartIndexPhase::FinishSpool;
        }
        Ok(())
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<ReferenceRestartIndexProgress, RestartIndexError> {
        if session.id() != self.authority.build {
            return Err(RestartIndexError::Invalid(
                "restart index poll crossed arena build authority",
            ));
        }
        match self.phase {
            ReferenceRestartIndexPhase::Ready => {
                return Ok(ReferenceRestartIndexProgress::ReadyForOccurrence);
            }
            ReferenceRestartIndexPhase::OccurrenceAckReady => {
                return Ok(ReferenceRestartIndexProgress::OccurrenceAckReady);
            }
            ReferenceRestartIndexPhase::ManifestReady => {
                return Ok(ReferenceRestartIndexProgress::ManifestReady);
            }
            ReferenceRestartIndexPhase::Taken | ReferenceRestartIndexPhase::Failed => {
                return Err(RestartIndexError::Invalid(
                    "restart index builder is consumed or poisoned",
                ));
            }
            _ => {}
        }
        let branches_before = self.sequence_receipt.branches_allocated;
        let query_before = self.query_receipt.sequence_nodes_visited;
        let reverse_before = self
            .reverse_cursor
            .as_ref()
            .map_or(0, |cursor| cursor.pages_read);
        let result = self.poll_one(session);
        let branches_this_task = self
            .sequence_receipt
            .branches_allocated
            .saturating_sub(branches_before);
        let query_this_task = self
            .query_receipt
            .sequence_nodes_visited
            .saturating_sub(query_before);
        let reverse_after = self
            .reverse_cursor
            .as_ref()
            .map_or(reverse_before, |cursor| cursor.pages_read);
        let reverse_this_task = reverse_after.saturating_sub(reverse_before);
        if branches_this_task > 1
            || query_this_task > MAX_QUERY_SEQUENCE_NODES_PER_TASK
            || reverse_this_task > 1
        {
            self.phase = ReferenceRestartIndexPhase::Failed;
            return Err(RestartIndexError::Invalid(
                "restart index task exceeded its bounded work envelope",
            ));
        }
        self.receipt.bounded_tasks = self
            .receipt
            .bounded_tasks
            .checked_add(1)
            .ok_or(RestartIndexError::Invalid("restart task receipt overflow"))?;
        self.receipt.sequence_branches_allocated = self.sequence_receipt.branches_allocated;
        self.receipt.maximum_branches_per_task = self
            .receipt
            .maximum_branches_per_task
            .max(branches_this_task);
        self.receipt.query_sequence_nodes_visited = self.query_receipt.sequence_nodes_visited;
        self.receipt.maximum_query_sequence_nodes_per_task = self
            .receipt
            .maximum_query_sequence_nodes_per_task
            .max(query_this_task);
        self.receipt.reverse_cursor_pages_read = reverse_after;
        self.receipt.maximum_reverse_cursor_pages_per_task = self
            .receipt
            .maximum_reverse_cursor_pages_per_task
            .max(reverse_this_task);
        self.receipt.maximum_live_owners =
            self.receipt.maximum_live_owners.max(session.live_owners()?);
        if let Err(error) = result {
            self.phase = ReferenceRestartIndexPhase::Failed;
            return Err(error);
        }
        if self.fault_after_task == Some(self.receipt.bounded_tasks) {
            self.phase = ReferenceRestartIndexPhase::Failed;
            return Err(RestartIndexError::InjectedFault(self.receipt.bounded_tasks));
        }
        Ok(match self.phase {
            ReferenceRestartIndexPhase::Ready => ReferenceRestartIndexProgress::ReadyForOccurrence,
            ReferenceRestartIndexPhase::OccurrenceAckReady => {
                ReferenceRestartIndexProgress::OccurrenceAckReady
            }
            ReferenceRestartIndexPhase::ManifestReady => {
                ReferenceRestartIndexProgress::ManifestReady
            }
            _ => ReferenceRestartIndexProgress::Pending,
        })
    }

    #[allow(clippy::too_many_lines)]
    fn poll_one(&mut self, session: &mut ArenaBuildSession<'_>) -> Result<(), RestartIndexError> {
        match self.phase {
            ReferenceRestartIndexPhase::AllocateSpoolLeaf => {
                let occurrence = self
                    .pending
                    .as_ref()
                    .ok_or(RestartIndexError::Invalid(
                        "restart occurrence disappeared before spool allocation",
                    ))?
                    .occurrence;
                let leaf = allocate_global_occurrence_leaf(session, occurrence)?;
                self.spool_builder
                    .begin_push(session, leaf, &mut self.sequence_receipt)?;
                self.phase = ReferenceRestartIndexPhase::ReleaseDestinationBlob;
            }
            ReferenceRestartIndexPhase::ReleaseDestinationBlob => {
                if let Some(owner) = self
                    .pending
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "restart occurrence disappeared before destination release",
                    ))?
                    .destination
                    .take()
                {
                    session.release(owner)?;
                }
                self.phase = ReferenceRestartIndexPhase::ReleaseTitleBlob;
            }
            ReferenceRestartIndexPhase::ReleaseTitleBlob => {
                if let Some(owner) = self
                    .pending
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "restart occurrence disappeared before title release",
                    ))?
                    .title
                    .take()
                {
                    session.release(owner)?;
                }
                self.phase = ReferenceRestartIndexPhase::PushSpoolLeaf;
            }
            ReferenceRestartIndexPhase::PushSpoolLeaf => {
                if self
                    .spool_builder
                    .poll_push(session, &mut self.sequence_receipt)?
                    == ResumableSequenceProgress::Complete
                {
                    let mut pending = self.pending.take().ok_or(RestartIndexError::Invalid(
                        "spooled restart occurrence disappeared",
                    ))?;
                    self.replacement_count =
                        self.replacement_count
                            .checked_add(1)
                            .ok_or(RestartIndexError::Invalid(
                                "replacement occurrence count overflow",
                            ))?;
                    self.receipt.occurrences_spooled = self.replacement_count;
                    self.ready_ack = Some(ReferenceCandidateOccurrenceAck {
                        label_use: pending.label_use.take().ok_or(RestartIndexError::Invalid(
                            "spooled restart occurrence lost its label-use acknowledgement",
                        ))?,
                    });
                    self.phase = ReferenceRestartIndexPhase::OccurrenceAckReady;
                }
            }
            ReferenceRestartIndexPhase::FinishSpool => {
                if self
                    .spool_builder
                    .poll_finish(session, &mut self.sequence_receipt)?
                    == ResumableSequenceProgress::Complete
                {
                    self.spool_splice_owner = Some(self.spool_builder.take_root()?);
                    self.phase = ReferenceRestartIndexPhase::AdoptSpoolRoot;
                }
            }
            ReferenceRestartIndexPhase::AdoptSpoolRoot => {
                let root = session.owner_id(self.spool_splice_owner.as_ref().ok_or(
                    RestartIndexError::Invalid("replacement spool root disappeared"),
                )?)?;
                self.spool_replay_owner = Some(session.retain(root)?);
                self.reverse_cursor = Some(PersistentReplacementReverseCursor::try_new(
                    session.arena(),
                    Some(root),
                    self.replacement_count,
                )?);
                self.phase = ReferenceRestartIndexPhase::RetainDonorGlobal;
            }
            ReferenceRestartIndexPhase::RetainDonorGlobal => {
                if self.replacement_count == 0 && self.reverse_cursor.is_none() {
                    self.reverse_cursor = Some(PersistentReplacementReverseCursor::try_new(
                        session.arena(),
                        None,
                        0,
                    )?);
                }
                let working = self
                    .authority
                    .donor_manifest
                    .global_root
                    .map(|root| session.retain(root))
                    .transpose()?;
                let mut splice = ResumableSequenceSplice::<GlobalOccurrenceSpec>::try_from_owned(
                    session,
                    working,
                    self.authority.checkpoint.occurrence_high_water
                        ..self.authority.old_join_high_water,
                    self.spool_splice_owner.take(),
                    &mut self.sequence_receipt,
                )?;
                if splice.poll(session, &mut self.sequence_receipt)?
                    == ResumableSequenceSplitProgress::Complete
                {
                    self.global_splice = Some(splice);
                    self.phase = ReferenceRestartIndexPhase::TakeGlobalRoot;
                } else {
                    self.global_splice = Some(splice);
                    self.phase = ReferenceRestartIndexPhase::PollGlobalSplice;
                }
            }
            ReferenceRestartIndexPhase::PollGlobalSplice => {
                let splice = self
                    .global_splice
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "restart global splice disappeared",
                    ))?;
                if splice.poll(session, &mut self.sequence_receipt)?
                    == ResumableSequenceSplitProgress::Complete
                {
                    self.phase = ReferenceRestartIndexPhase::TakeGlobalRoot;
                }
            }
            ReferenceRestartIndexPhase::TakeGlobalRoot => {
                self.global_root = self
                    .global_splice
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "completed restart global splice disappeared",
                    ))?
                    .take_root()?;
                self.phase = ReferenceRestartIndexPhase::RetainDonorDirectory;
            }
            ReferenceRestartIndexPhase::RetainDonorDirectory => {
                self.working_directory = self
                    .authority
                    .donor_manifest
                    .directory_root
                    .map(|root| session.retain(root))
                    .transpose()?;
                self.phase = ReferenceRestartIndexPhase::StartOldDeletion;
            }
            ReferenceRestartIndexPhase::StartOldDeletion => {
                if self.old_cursor == self.authority.old_join_high_water {
                    self.phase = ReferenceRestartIndexPhase::PollReplacementReverse;
                    return Ok(());
                }
                let occurrence = global_occurrence_at_counted(
                    session.arena(),
                    self.authority.donor_manifest.global_root,
                    self.old_cursor,
                    &mut self.query_receipt,
                )?
                .1;
                self.active_label = Some(ActiveLabelMutation::try_new(
                    session,
                    self.authority.checkpoint.prefix_directory,
                    self.working_directory.take(),
                    LabelMutationKind::Delete {
                        expected: occurrence,
                    },
                    &mut self.query_receipt,
                )?);
                self.active_label_kind = Some(ActiveRestartLabelMutationKind::Delete);
                self.phase = ReferenceRestartIndexPhase::PollLabelMutation;
            }
            ReferenceRestartIndexPhase::PollReplacementReverse => {
                match self
                    .reverse_cursor
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "replacement reverse cursor disappeared",
                    ))?
                    .poll(session.arena())?
                {
                    ReverseSpoolProgress::Pending => {}
                    ReverseSpoolProgress::Complete => {
                        self.phase = ReferenceRestartIndexPhase::AllocateGreen;
                    }
                    ReverseSpoolProgress::Item {
                        descriptor,
                        occurrence,
                    } => {
                        self.active_label = Some(ActiveLabelMutation::try_new(
                            session,
                            self.authority.checkpoint.prefix_directory,
                            self.working_directory.take(),
                            LabelMutationKind::Insert {
                                occurrence,
                                descriptor,
                            },
                            &mut self.query_receipt,
                        )?);
                        self.active_label_kind = Some(ActiveRestartLabelMutationKind::Insert);
                        self.phase = ReferenceRestartIndexPhase::PollLabelMutation;
                    }
                }
            }
            ReferenceRestartIndexPhase::PollLabelMutation => {
                let mutation = self
                    .active_label
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "restart label mutation disappeared",
                    ))?;
                if mutation.poll(session, &mut self.sequence_receipt)? {
                    self.working_directory = mutation.take_directory()?;
                    self.active_label = None;
                    match self
                        .active_label_kind
                        .take()
                        .ok_or(RestartIndexError::Invalid(
                            "restart label mutation kind disappeared",
                        ))? {
                        ActiveRestartLabelMutationKind::Delete => {
                            self.old_cursor = self
                                .old_cursor
                                .checked_add(1)
                                .ok_or(RestartIndexError::Invalid("old restart cursor overflow"))?;
                            self.receipt.old_changed_occurrences_read = self
                                .receipt
                                .old_changed_occurrences_read
                                .checked_add(1)
                                .ok_or(RestartIndexError::Invalid(
                                    "old restart occurrence receipt overflow",
                                ))?;
                            self.phase = ReferenceRestartIndexPhase::StartOldDeletion;
                        }
                        ActiveRestartLabelMutationKind::Insert => {
                            self.receipt.replacement_occurrences_reverse_read = self
                                .receipt
                                .replacement_occurrences_reverse_read
                                .checked_add(1)
                                .ok_or(RestartIndexError::Invalid(
                                    "replacement reverse receipt overflow",
                                ))?;
                            self.phase = ReferenceRestartIndexPhase::PollReplacementReverse;
                        }
                    }
                }
            }
            ReferenceRestartIndexPhase::AllocateGreen => {
                let (green, _) = session.allocate_packed(
                    &encode_green(
                        self.authority.candidate_source.revision.0,
                        self.authority.green_generation,
                        self.authority.authority_nonce,
                    ),
                    &[],
                )?;
                self.green = Some(green);
                self.phase = ReferenceRestartIndexPhase::AllocateCheckpoint;
            }
            ReferenceRestartIndexPhase::AllocateCheckpoint => {
                let green_id = session.owner_id(self.green.as_ref().ok_or(
                    RestartIndexError::Invalid("restart Green authority disappeared"),
                )?)?;
                let payload = encode_checkpoint_manifest(
                    self.authority.candidate_source.revision.0,
                    self.authority.checkpoint.occurrence_high_water,
                    self.adopted_interner_generation,
                    self.authority.source_lineage_generation,
                    self.authority.checkpoint.prefix_directory.is_some(),
                );
                let (checkpoint, _) =
                    if let Some(prefix) = self.authority.checkpoint.prefix_directory {
                        session.allocate_packed(&payload, &[prefix, green_id])?
                    } else {
                        session.allocate_packed(&payload, &[green_id])?
                    };
                self.checkpoint_id = Some(session.owner_id(&checkpoint)?);
                self.checkpoint_owner = Some(checkpoint);
                self.phase = ReferenceRestartIndexPhase::AllocateInternerAuthority;
            }
            ReferenceRestartIndexPhase::AllocateInternerAuthority => {
                let interner =
                    self.interner_manifest
                        .as_ref()
                        .ok_or(RestartIndexError::Invalid(
                            "restart interner manifest disappeared",
                        ))?;
                let interner_id = session.owner_id(&interner.owner)?;
                let (authority, _) = session.allocate_packed(
                    &encode_interner_authority(
                        interner.identity.generation,
                        interner.identity.label_id_high_water,
                        true,
                    ),
                    &[interner_id],
                )?;
                self.interner_authority = Some(authority);
                self.phase = ReferenceRestartIndexPhase::ReleaseInternerManifest;
            }
            ReferenceRestartIndexPhase::ReleaseInternerManifest => {
                session.release(
                    self.interner_manifest
                        .take()
                        .ok_or(RestartIndexError::Invalid(
                            "restart interner manifest disappeared before release",
                        ))?
                        .owner,
                )?;
                self.phase = ReferenceRestartIndexPhase::AllocateSourceLineage;
            }
            ReferenceRestartIndexPhase::AllocateSourceLineage => {
                let (lineage, _) = session.allocate_packed(
                    &encode_source_lineage_authority(
                        self.authority.source_lineage_generation,
                        self.authority.candidate_source,
                    )?,
                    &[],
                )?;
                self.source_lineage = Some(lineage);
                self.phase = ReferenceRestartIndexPhase::AllocateOccurrenceAllocator;
            }
            ReferenceRestartIndexPhase::AllocateOccurrenceAllocator => {
                let generation = self
                    .authority
                    .donor_manifest
                    .occurrence_allocator_generation
                    .checked_add(1)
                    .ok_or(RestartIndexError::Invalid(
                        "restart occurrence allocator generation overflow",
                    ))?;
                let high_water = self
                    .authority
                    .donor_manifest
                    .occurrence_allocator_high_water
                    .checked_add(self.replacement_count)
                    .ok_or(RestartIndexError::Invalid(
                        "restart occurrence allocator high-water overflow",
                    ))?;
                let (allocator, _) = session
                    .allocate_packed(&encode_occurrence_allocator(generation, high_water), &[])?;
                self.occurrence_allocator = Some(allocator);
                self.phase = ReferenceRestartIndexPhase::AllocateDonorRange;
            }
            ReferenceRestartIndexPhase::AllocateDonorRange => {
                let donor = DonorRangeManifest {
                    donor_document_root: session.arena().local_id(self.authority.donor.root)?,
                    donor_global_root: self.authority.donor_manifest.global_root,
                    donor_directory_root: self.authority.donor_manifest.directory_root,
                    donor_checkpoint_root: self.authority.donor_manifest.checkpoint_root,
                    donor_interner_root: self.authority.donor_manifest.interner_root,
                    donor_green_root: self.authority.donor_manifest.green_root,
                    donor_occurrence_allocator_root: self
                        .authority
                        .donor_manifest
                        .occurrence_allocator_root,
                    donor_source_revision: self.authority.donor_manifest.source_revision,
                    old_range: self.authority.checkpoint.occurrence_high_water
                        ..self.authority.old_join_high_water,
                    donor_occurrence_count: self.authority.donor_manifest.occurrence_count,
                    interner_generation: self.authority.donor_manifest.interner_generation,
                    source_lineage_generation: self
                        .authority
                        .donor_manifest
                        .source_lineage_generation,
                    source_lineage_root: self.authority.donor_manifest.source_lineage_root,
                    occurrence_allocator_generation: self
                        .authority
                        .donor_manifest
                        .occurrence_allocator_generation,
                    occurrence_allocator_high_water: self
                        .authority
                        .donor_manifest
                        .occurrence_allocator_high_water,
                };
                let children = [
                    donor.donor_checkpoint_root,
                    donor.donor_interner_root,
                    donor.source_lineage_root,
                    donor.donor_green_root,
                    donor.donor_occurrence_allocator_root,
                ];
                let (owner, _) = session.allocate_packed(&encode_donor_range(&donor), &children)?;
                self.donor_range = Some(owner);
                self.phase = ReferenceRestartIndexPhase::AllocateSuffixAdoption;
            }
            ReferenceRestartIndexPhase::AllocateSuffixAdoption => {
                let donor_range = session.owner_id(self.donor_range.as_ref().ok_or(
                    RestartIndexError::Invalid("restart donor range disappeared"),
                )?)?;
                let source_lineage = session.owner_id(self.source_lineage.as_ref().ok_or(
                    RestartIndexError::Invalid("restart source lineage disappeared"),
                )?)?;
                let candidate_green = session.owner_id(self.green.as_ref().ok_or(
                    RestartIndexError::Invalid("restart candidate Green disappeared"),
                )?)?;
                let payload = encode_suffix_adoption(
                    self.authority.donor_manifest.source_revision,
                    self.authority.candidate_source.revision.0,
                    self.authority.old_join_high_water
                        ..self.authority.donor_manifest.occurrence_count,
                    self.authority
                        .checkpoint
                        .occurrence_high_water
                        .checked_add(self.replacement_count)
                        .ok_or(RestartIndexError::Invalid("restart suffix start overflow"))?,
                    self.authority.source_lineage_generation,
                );
                let (adoption, _) = session.allocate_packed(
                    &payload,
                    &[
                        donor_range,
                        source_lineage,
                        self.authority.checkpoint.green_root,
                        candidate_green,
                    ],
                )?;
                self.adoption = Some(adoption);
                self.phase = ReferenceRestartIndexPhase::AllocateDocument;
            }
            ReferenceRestartIndexPhase::AllocateDocument => {
                let deleted = self
                    .authority
                    .old_join_high_water
                    .checked_sub(self.authority.checkpoint.occurrence_high_water)
                    .ok_or(RestartIndexError::Invalid("restart range is reversed"))?;
                let occurrence_count = self
                    .authority
                    .donor_manifest
                    .occurrence_count
                    .checked_sub(deleted)
                    .and_then(|count| count.checked_add(self.replacement_count))
                    .ok_or(RestartIndexError::Invalid(
                        "restart occurrence count overflow",
                    ))?;
                let global_id = self
                    .global_root
                    .as_ref()
                    .map(|owner| session.owner_id(owner))
                    .transpose()?;
                let directory_id = self
                    .working_directory
                    .as_ref()
                    .map(|owner| session.owner_id(owner))
                    .transpose()?;
                let checkpoint_id = self.checkpoint_id.ok_or(RestartIndexError::Invalid(
                    "restart checkpoint identity disappeared",
                ))?;
                let interner_id = session.owner_id(self.interner_authority.as_ref().ok_or(
                    RestartIndexError::Invalid("restart interner authority disappeared"),
                )?)?;
                let lineage_id = session.owner_id(self.source_lineage.as_ref().ok_or(
                    RestartIndexError::Invalid("restart lineage authority disappeared"),
                )?)?;
                let green_id = session.owner_id(self.green.as_ref().ok_or(
                    RestartIndexError::Invalid("restart Green authority disappeared"),
                )?)?;
                let allocator_id = session.owner_id(self.occurrence_allocator.as_ref().ok_or(
                    RestartIndexError::Invalid("restart allocator authority disappeared"),
                )?)?;
                let adoption_id = session.owner_id(self.adoption.as_ref().ok_or(
                    RestartIndexError::Invalid("restart adoption authority disappeared"),
                )?)?;
                let mut children = Vec::with_capacity(DOCUMENT_BASE_CHILDREN + 1);
                children.extend(global_id);
                children.extend(directory_id);
                children.extend([
                    checkpoint_id,
                    interner_id,
                    lineage_id,
                    green_id,
                    allocator_id,
                    adoption_id,
                ]);
                let allocator_generation = self
                    .authority
                    .donor_manifest
                    .occurrence_allocator_generation
                    .checked_add(1)
                    .ok_or(RestartIndexError::Invalid(
                        "restart allocator generation overflow",
                    ))?;
                let allocator_high_water = self
                    .authority
                    .donor_manifest
                    .occurrence_allocator_high_water
                    .checked_add(self.replacement_count)
                    .ok_or(RestartIndexError::Invalid(
                        "restart allocator high-water overflow",
                    ))?;
                let payload = encode_document_manifest(
                    self.authority.candidate_source.revision.0,
                    self.authority.donor_manifest.source_revision,
                    occurrence_count,
                    self.authority.checkpoint.occurrence_high_water,
                    self.authority.old_join_high_water,
                    self.replacement_count,
                    self.adopted_interner_generation,
                    self.authority.source_lineage_generation,
                    allocator_generation,
                    allocator_high_water,
                    Some(self.authority.candidate_source),
                    global_id.is_some(),
                    directory_id.is_some(),
                    true,
                )?;
                let (terminal, _) = session.allocate_packed(&payload, &children)?;
                self.receipt.manifest_children_joined = children.len();
                self.terminal = Some(terminal);
                self.phase = ReferenceRestartIndexPhase::ReleaseGlobal;
            }
            ReferenceRestartIndexPhase::ReleaseGlobal => {
                if let Some(owner) = self.global_root.take() {
                    session.release(owner)?;
                }
                self.phase = ReferenceRestartIndexPhase::ReleaseDirectory;
            }
            ReferenceRestartIndexPhase::ReleaseDirectory => {
                if let Some(owner) = self.working_directory.take() {
                    session.release(owner)?;
                }
                self.phase = ReferenceRestartIndexPhase::ReleaseSpoolReplay;
            }
            ReferenceRestartIndexPhase::ReleaseSpoolReplay => {
                if let Some(owner) = self.spool_replay_owner.take() {
                    session.release(owner)?;
                }
                self.phase = ReferenceRestartIndexPhase::ReleaseCheckpoint;
            }
            ReferenceRestartIndexPhase::ReleaseCheckpoint => {
                session.release(self.checkpoint_owner.take().ok_or(
                    RestartIndexError::Invalid("restart checkpoint owner disappeared"),
                )?)?;
                self.phase = ReferenceRestartIndexPhase::ReleaseInterner;
            }
            ReferenceRestartIndexPhase::ReleaseInterner => {
                session.release(self.interner_authority.take().ok_or(
                    RestartIndexError::Invalid("restart interner owner disappeared"),
                )?)?;
                self.phase = ReferenceRestartIndexPhase::ReleaseSourceLineage;
            }
            ReferenceRestartIndexPhase::ReleaseSourceLineage => {
                session.release(self.source_lineage.take().ok_or(
                    RestartIndexError::Invalid("restart source-lineage owner disappeared"),
                )?)?;
                self.phase = ReferenceRestartIndexPhase::ReleaseGreen;
            }
            ReferenceRestartIndexPhase::ReleaseGreen => {
                session.release(self.green.take().ok_or(RestartIndexError::Invalid(
                    "restart Green owner disappeared",
                ))?)?;
                self.phase = ReferenceRestartIndexPhase::ReleaseOccurrenceAllocator;
            }
            ReferenceRestartIndexPhase::ReleaseOccurrenceAllocator => {
                session.release(self.occurrence_allocator.take().ok_or(
                    RestartIndexError::Invalid("restart allocator owner disappeared"),
                )?)?;
                self.phase = ReferenceRestartIndexPhase::ReleaseDonorRange;
            }
            ReferenceRestartIndexPhase::ReleaseDonorRange => {
                session.release(self.donor_range.take().ok_or(RestartIndexError::Invalid(
                    "restart donor-range owner disappeared",
                ))?)?;
                self.phase = ReferenceRestartIndexPhase::ReleaseAdoption;
            }
            ReferenceRestartIndexPhase::ReleaseAdoption => {
                session.release(self.adoption.take().ok_or(RestartIndexError::Invalid(
                    "restart adoption owner disappeared",
                ))?)?;
                if session.live_owners()? != 1 {
                    return Err(RestartIndexError::Invalid(
                        "restart index did not reduce to one terminal owner",
                    ));
                }
                self.phase = ReferenceRestartIndexPhase::ManifestReady;
            }
            ReferenceRestartIndexPhase::Ready
            | ReferenceRestartIndexPhase::OccurrenceAckReady
            | ReferenceRestartIndexPhase::ManifestReady
            | ReferenceRestartIndexPhase::Taken
            | ReferenceRestartIndexPhase::Failed => {
                return Err(RestartIndexError::Invalid(
                    "restart index phase is not pollable",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn take_manifest(
        &mut self,
    ) -> Result<ReferenceRestartIndexManifest, RestartIndexError> {
        if self.phase != ReferenceRestartIndexPhase::ManifestReady {
            return Err(RestartIndexError::Invalid(
                "restart index manifest is not ready",
            ));
        }
        let manifest = ReferenceRestartIndexManifest {
            owner: Some(self.terminal.take().ok_or(RestartIndexError::Invalid(
                "restart terminal owner disappeared",
            ))?),
            checkpoint: self.checkpoint_id.ok_or(RestartIndexError::Invalid(
                "restart checkpoint identity disappeared",
            ))?,
            receipt: self.receipt,
        };
        self.phase = ReferenceRestartIndexPhase::Taken;
        Ok(manifest)
    }
}

/// Parser-finalizer capability proving that the edit starts after this exact
/// checkpoint and that convergence joined this exact donor semantic index.
/// The update job accepts this typed value, never a caller-selected Range.
#[derive(Debug)]
struct AuthenticatedConvergenceRange {
    donor_document_root: ArenaScopedId,
    checkpoint_root: ArenaScopedId,
    donor_manifest: DocumentManifest,
    checkpoint: CheckpointManifest,
    old_range: Range<u64>,
}

impl AuthenticatedConvergenceRange {
    fn try_mint(
        arena: &PageArena,
        donor: &RestartIndexDocument,
        edit_occurrence_start: u64,
        old_join_high_water: u64,
    ) -> Result<Self, RestartIndexError> {
        let donor_document_root = donor.root()?;
        let donor_manifest = donor.manifest(arena)?;
        let checkpoint_root = donor.checkpoint();
        let checkpoint_id = arena.local_id(checkpoint_root)?;
        if checkpoint_id != donor_manifest.checkpoint_root {
            return Err(RestartIndexError::Invalid(
                "convergence checkpoint crossed its donor document",
            ));
        }
        let checkpoint = decode_checkpoint_manifest(arena, checkpoint_id)?;
        if checkpoint.source_revision != donor_manifest.source_revision
            || checkpoint.occurrence_high_water != donor_manifest.restart_high_water
            || edit_occurrence_start < checkpoint.occurrence_high_water
            || old_join_high_water < edit_occurrence_start
            || old_join_high_water > donor_manifest.occurrence_count
        {
            return Err(RestartIndexError::Invalid(
                "parser convergence is outside its authenticated checkpoint/donor range",
            ));
        }
        for root in [
            donor_manifest.global_root,
            donor_manifest.directory_root,
            checkpoint.prefix_directory,
        ]
        .into_iter()
        .flatten()
        {
            let height = if Some(root) == donor_manifest.global_root {
                GlobalOccurrenceSpec::height(sequence_node::<GlobalOccurrenceSpec>(arena, root)?.0)
            } else if Some(root) == donor_manifest.directory_root
                || Some(root) == checkpoint.prefix_directory
            {
                ExactLabelDirectorySpec::height(
                    sequence_node::<ExactLabelDirectorySpec>(arena, root)?.0,
                )
            } else {
                unreachable!("enumerated authenticated root kind")
            };
            if height > MAX_AUTHENTICATED_SEQUENCE_HEIGHT {
                return Err(RestartIndexError::Invalid(
                    "persistent sequence exceeds the authenticated query-height envelope",
                ));
            }
        }
        Ok(Self {
            donor_document_root,
            checkpoint_root,
            donor_manifest,
            checkpoint,
            old_range: checkpoint.occurrence_high_water..old_join_high_water,
        })
    }
}

/// Non-cloneable writer seal for one candidate revision.  It binds every
/// draft to the exact donor, interner, stable source/projection lineage, and
/// occurrence-ID allocator from which it was minted.
#[derive(Debug)]
struct CandidateSourceSeal {
    authority_nonce: u64,
    donor_document_root: ArenaScopedId,
    parent_source_revision: u64,
    candidate_source_revision: u64,
    interner_root: ArenaScopedId,
    source_lineage_root: ArenaScopedId,
    occurrence_allocator_root: ArenaScopedId,
    occurrence_allocator_generation: u64,
    occurrence_allocator_high_water: u64,
}

#[derive(Debug)]
struct WriterOccurrenceDraft {
    authority_nonce: u64,
    candidate_source_revision: u64,
    interner_root: ArenaScopedId,
    source_lineage_root: ArenaScopedId,
    occurrence_allocator_root: ArenaScopedId,
    occurrence_allocator_generation: u64,
    occurrence: ReferenceOccurrence,
}

#[derive(Debug)]
struct CandidateChangeSet {
    source: CandidateSourceSeal,
    drafts: Vec<WriterOccurrenceDraft>,
}

#[derive(Debug)]
struct RestartWriterAuthority {
    source: CandidateSourceSeal,
    next_occurrence_id: u64,
}

impl RestartWriterAuthority {
    fn from_document(
        arena: &PageArena,
        document: &RestartIndexDocument,
        candidate_source_revision: u64,
        authority_nonce: u64,
    ) -> Result<Self, RestartIndexError> {
        if authority_nonce == 0 {
            return Err(RestartIndexError::Invalid(
                "candidate occurrence authority is incomplete",
            ));
        }
        let manifest = document.manifest(arena)?;
        if candidate_source_revision <= manifest.source_revision {
            return Err(RestartIndexError::Invalid(
                "candidate source revision did not advance",
            ));
        }
        let next_occurrence_id = manifest
            .occurrence_allocator_high_water
            .checked_add(1)
            .ok_or(RestartIndexError::Invalid(
                "occurrence identity allocator is exhausted",
            ))?;
        Ok(Self {
            source: CandidateSourceSeal {
                authority_nonce,
                donor_document_root: document.root()?,
                parent_source_revision: manifest.source_revision,
                candidate_source_revision,
                interner_root: arena.scoped_query_id(manifest.interner_root)?,
                source_lineage_root: arena.scoped_query_id(manifest.source_lineage_root)?,
                occurrence_allocator_root: arena
                    .scoped_query_id(manifest.occurrence_allocator_root)?,
                occurrence_allocator_generation: manifest.occurrence_allocator_generation,
                occurrence_allocator_high_water: manifest.occurrence_allocator_high_water,
            },
            next_occurrence_id,
        })
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(test)]
    fn occurrence(
        &mut self,
        label_id: u64,
        destination_id: u64,
        source_piece: u64,
        source_piece_offset: u64,
        source_length: u64,
        projection_program: u64,
        projection_logical_offset: u64,
    ) -> Result<WriterOccurrenceDraft, RestartIndexError> {
        let occurrence_id = self.next_occurrence_id;
        self.next_occurrence_id =
            self.next_occurrence_id
                .checked_add(1)
                .ok_or(RestartIndexError::Invalid(
                    "occurrence identity allocator is exhausted",
                ))?;
        let occurrence = ReferenceOccurrence {
            occurrence_id,
            label_id,
            value: ReferenceOccurrenceValue::FixtureLegacy {
                destination_id,
                coordinate_generation: self.source.candidate_source_revision,
                coordinate: FixtureRetainedOccurrenceCoordinate {
                    source_piece,
                    source_piece_offset,
                    source_length,
                    projection_program,
                    projection_logical_offset,
                },
            },
        };
        occurrence.validate()?;
        Ok(WriterOccurrenceDraft {
            authority_nonce: self.source.authority_nonce,
            candidate_source_revision: self.source.candidate_source_revision,
            interner_root: self.source.interner_root,
            source_lineage_root: self.source.source_lineage_root,
            occurrence_allocator_root: self.source.occurrence_allocator_root,
            occurrence_allocator_generation: self.source.occurrence_allocator_generation,
            occurrence,
        })
    }

    fn finish(self, drafts: Vec<WriterOccurrenceDraft>) -> CandidateChangeSet {
        CandidateChangeSet {
            source: self.source,
            drafts,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RestartUpdateReceipt {
    bounded_tasks: u64,
    old_changed_occurrences_read: u64,
    new_changed_occurrences_read: u64,
    suffix_occurrences_enumerated: u64,
    global_leaves_reused: usize,
    sequence_branches_allocated: usize,
    maximum_branches_per_task: usize,
    maximum_live_owners: usize,
    manifest_children_joined: usize,
    query_sequence_nodes_visited: u64,
    maximum_query_sequence_nodes_per_task: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestartUpdateProgress {
    Pending,
    Committable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestartUpdatePhase {
    AllocateGlobalLeaf,
    PushGlobalLeaf,
    FinishGlobal,
    RetainOldGlobal,
    PollGlobalSplice,
    TakeGlobal,
    RetainOldDirectory,
    StartOldDeletion,
    PollLabelMutation,
    StartNewInsertion,
    AllocateCandidateGreen,
    AllocateCandidateCheckpoint,
    RetainInterner,
    RetainSourceLineage,
    AllocateOccurrenceAllocator,
    AllocateDonorRange,
    AllocateSuffixAdoption,
    AllocateManifest,
    ReleaseGlobal,
    ReleaseDirectory,
    ReleaseCandidateCheckpoint,
    ReleaseInterner,
    ReleaseSourceLineage,
    ReleaseGreen,
    ReleaseOccurrenceAllocator,
    ReleaseDonorRange,
    ReleaseAdoption,
    Committable,
    Failed,
}

#[derive(Debug)]
struct RestartIndexUpdateJob {
    build: crate::ArenaBuildId,
    old_manifest: DocumentManifest,
    donor_document_root: ArenaScopedId,
    checkpoint: CheckpointManifest,
    old_range: Range<u64>,
    candidate_source: CandidateSourceSeal,
    new_source_revision: u64,
    new_occurrences: Vec<ReferenceOccurrence>,
    new_global_builder: Option<ResumableStreamingSequenceBuilder<GlobalOccurrenceSpec>>,
    pending_global_leaf: Option<ArenaBuildOwner>,
    new_global_descriptor_ids: Vec<ArenaId>,
    new_global_index: usize,
    new_global_replacement: Option<ArenaBuildOwner>,
    global_splice: Option<ResumableSequenceSplice<GlobalOccurrenceSpec>>,
    global_root: Option<ArenaBuildOwner>,
    working_directory: Option<ArenaBuildOwner>,
    active_label: Option<ActiveLabelMutation>,
    old_cursor: u64,
    new_reverse_cursor: usize,
    candidate_green: Option<ArenaBuildOwner>,
    candidate_checkpoint_owner: Option<ArenaBuildOwner>,
    candidate_checkpoint_id: Option<ArenaId>,
    interner_owner: Option<ArenaBuildOwner>,
    source_lineage_owner: Option<ArenaBuildOwner>,
    occurrence_allocator_owner: Option<ArenaBuildOwner>,
    donor_range_owner: Option<ArenaBuildOwner>,
    adoption_owner: Option<ArenaBuildOwner>,
    terminal_owner: Option<ArenaBuildOwner>,
    phase: RestartUpdatePhase,
    fault_after_task: Option<u64>,
    sequence_receipt: SequenceMutationReceipt,
    query_receipt: QueryWorkReceipt,
    receipt: RestartUpdateReceipt,
}

impl RestartIndexUpdateJob {
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn try_new(
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        old_document: &RestartIndexDocument,
        convergence: AuthenticatedConvergenceRange,
        change_set: CandidateChangeSet,
    ) -> Result<Self, RestartIndexError> {
        let old_manifest = old_document.manifest(arena)?;
        let donor_document_root = old_document.root()?;
        let checkpoint_root = convergence.checkpoint_root;
        let local_checkpoint = arena.local_id(checkpoint_root)?;
        if donor_document_root != convergence.donor_document_root
            || convergence.donor_manifest != old_manifest
            || convergence.checkpoint != decode_checkpoint_manifest(arena, local_checkpoint)?
            || local_checkpoint != old_manifest.checkpoint_root
            || checkpoint_root != old_document.checkpoint()
        {
            return Err(RestartIndexError::Invalid(
                "authenticated convergence crossed the selected donor document",
            ));
        }
        let checkpoint = convergence.checkpoint;
        let old_range = convergence.old_range;
        let CandidateChangeSet { source, drafts } = change_set;
        let new_source_revision = source.candidate_source_revision;
        if checkpoint.source_revision != old_manifest.source_revision
            || checkpoint.interner_generation != old_manifest.interner_generation
            || checkpoint.source_lineage_generation != old_manifest.source_lineage_generation
            || checkpoint.occurrence_high_water != old_range.start
            || old_range.start > old_range.end
            || old_range.end > old_manifest.occurrence_count
            || new_source_revision <= old_manifest.source_revision
        {
            return Err(RestartIndexError::Invalid(
                "restart checkpoint/range/source authority is inconsistent",
            ));
        }
        let expected_interner = arena.scoped_query_id(old_manifest.interner_root)?;
        let expected_lineage = arena.scoped_query_id(old_manifest.source_lineage_root)?;
        let expected_allocator = arena.scoped_query_id(old_manifest.occurrence_allocator_root)?;
        if source.authority_nonce == 0
            || source.donor_document_root != donor_document_root
            || source.parent_source_revision != old_manifest.source_revision
            || source.interner_root != expected_interner
            || source.source_lineage_root != expected_lineage
            || source.occurrence_allocator_root != expected_allocator
            || source.occurrence_allocator_generation
                != old_manifest.occurrence_allocator_generation
            || source.occurrence_allocator_high_water
                != old_manifest.occurrence_allocator_high_water
        {
            return Err(RestartIndexError::Invalid(
                "candidate source seal crossed donor/interner/source/allocator authority",
            ));
        }
        let mut new_occurrences = Vec::new();
        new_occurrences
            .try_reserve_exact(drafts.len())
            .map_err(|_| RestartIndexError::Invalid("new occurrence segment reservation failed"))?;
        let mut expected_occurrence_id = old_manifest
            .occurrence_allocator_high_water
            .checked_add(1)
            .ok_or(RestartIndexError::Invalid(
                "occurrence identity allocator is exhausted",
            ))?;
        for draft in drafts {
            if draft.authority_nonce != source.authority_nonce
                || draft.candidate_source_revision != new_source_revision
                || draft.interner_root != expected_interner
                || draft.source_lineage_root != expected_lineage
                || draft.occurrence_allocator_root != expected_allocator
                || draft.occurrence_allocator_generation
                    != old_manifest.occurrence_allocator_generation
                || draft
                    .occurrence
                    .crosses_writer_source_revision(new_source_revision)
                || draft.occurrence.occurrence_id != expected_occurrence_id
            {
                return Err(RestartIndexError::Invalid(
                    "new occurrence crossed writer/interner/source/allocator authority",
                ));
            }
            draft.occurrence.validate()?;
            new_occurrences.push(draft.occurrence);
            expected_occurrence_id =
                expected_occurrence_id
                    .checked_add(1)
                    .ok_or(RestartIndexError::Invalid(
                        "occurrence identity allocator is exhausted",
                    ))?;
        }

        let mut sequence_receipt = SequenceMutationReceipt::default();
        let new_global_builder = (!new_occurrences.is_empty())
            .then(|| {
                ResumableStreamingSequenceBuilder::<GlobalOccurrenceSpec>::try_new(
                    &mut sequence_receipt,
                )
            })
            .transpose()?;
        let mut new_global_descriptor_ids = Vec::new();
        new_global_descriptor_ids
            .try_reserve_exact(new_occurrences.len())
            .map_err(|_| RestartIndexError::Invalid("descriptor-ID reservation failed"))?;
        let new_reverse_cursor = new_occurrences.len();
        Ok(Self {
            build: ticket.id(),
            old_manifest,
            donor_document_root,
            checkpoint,
            old_range: old_range.clone(),
            candidate_source: source,
            new_source_revision,
            new_occurrences,
            new_global_builder,
            pending_global_leaf: None,
            new_global_descriptor_ids,
            new_global_index: 0,
            new_global_replacement: None,
            global_splice: None,
            global_root: None,
            working_directory: None,
            active_label: None,
            old_cursor: old_range.start,
            new_reverse_cursor,
            candidate_green: None,
            candidate_checkpoint_owner: None,
            candidate_checkpoint_id: None,
            interner_owner: None,
            source_lineage_owner: None,
            occurrence_allocator_owner: None,
            donor_range_owner: None,
            adoption_owner: None,
            terminal_owner: None,
            phase: if old_range.start == old_range.end && new_reverse_cursor == 0 {
                RestartUpdatePhase::AllocateGlobalLeaf
            } else {
                RestartUpdatePhase::AllocateGlobalLeaf
            },
            fault_after_task: None,
            sequence_receipt,
            query_receipt: QueryWorkReceipt::default(),
            receipt: RestartUpdateReceipt::default(),
        })
    }

    fn with_fault_after_task(mut self, task: u64) -> Self {
        self.fault_after_task = Some(task);
        self
    }

    const fn receipt(&self) -> RestartUpdateReceipt {
        self.receipt
    }

    #[allow(clippy::too_many_lines)]
    fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<RestartUpdateProgress, RestartIndexError> {
        if session.id() != self.build {
            return Err(RestartIndexError::Invalid(
                "restart update crossed arena build authority",
            ));
        }
        if self.phase == RestartUpdatePhase::Committable {
            return Ok(RestartUpdateProgress::Committable);
        }
        if self.phase == RestartUpdatePhase::Failed {
            return Err(RestartIndexError::Invalid("restart update is poisoned"));
        }
        let branches_before = self.sequence_receipt.branches_allocated;
        let query_nodes_before = self.query_receipt.sequence_nodes_visited;
        let result = self.poll_one(session);
        let branches_after = self.sequence_receipt.branches_allocated;
        let query_nodes_after = self.query_receipt.sequence_nodes_visited;
        let branches_this_task = branches_after.saturating_sub(branches_before);
        let query_nodes_this_task = query_nodes_after.saturating_sub(query_nodes_before);
        if branches_this_task > 1 {
            self.phase = RestartUpdatePhase::Failed;
            return Err(RestartIndexError::Invalid(
                "one restart task allocated multiple persistent branches",
            ));
        }
        if query_nodes_this_task > MAX_QUERY_SEQUENCE_NODES_PER_TASK {
            self.phase = RestartUpdatePhase::Failed;
            return Err(RestartIndexError::Invalid(
                "one restart planning task exceeded its authenticated query-work envelope",
            ));
        }
        self.receipt.bounded_tasks = self
            .receipt
            .bounded_tasks
            .checked_add(1)
            .ok_or(RestartIndexError::Invalid("restart task count overflow"))?;
        self.receipt.maximum_branches_per_task = self
            .receipt
            .maximum_branches_per_task
            .max(branches_this_task);
        self.receipt.sequence_branches_allocated = self.sequence_receipt.branches_allocated;
        self.receipt.query_sequence_nodes_visited = query_nodes_after;
        self.receipt.maximum_query_sequence_nodes_per_task = self
            .receipt
            .maximum_query_sequence_nodes_per_task
            .max(query_nodes_this_task);
        self.receipt.maximum_live_owners =
            self.receipt.maximum_live_owners.max(session.live_owners()?);
        result?;
        if self.fault_after_task == Some(self.receipt.bounded_tasks) {
            return Err(RestartIndexError::InjectedFault(self.receipt.bounded_tasks));
        }
        Ok(if self.phase == RestartUpdatePhase::Committable {
            RestartUpdateProgress::Committable
        } else {
            RestartUpdateProgress::Pending
        })
    }

    #[allow(clippy::too_many_lines)]
    fn poll_one(&mut self, session: &mut ArenaBuildSession<'_>) -> Result<(), RestartIndexError> {
        match self.phase {
            RestartUpdatePhase::AllocateGlobalLeaf => {
                if self.new_global_index == self.new_occurrences.len() {
                    if let Some(builder) = self.new_global_builder.as_mut() {
                        builder.begin_finish(&mut self.sequence_receipt)?;
                        self.phase = RestartUpdatePhase::FinishGlobal;
                    } else {
                        self.phase = RestartUpdatePhase::RetainOldGlobal;
                    }
                    return Ok(());
                }
                let occurrence = self.new_occurrences[self.new_global_index];
                let leaf = allocate_global_occurrence_leaf(session, occurrence)?;
                let leaf_id = session.owner_id(&leaf)?;
                self.new_global_descriptor_ids.push(leaf_id);
                self.new_global_builder
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "new global sequence builder disappeared",
                    ))?
                    .begin_push(session, leaf, &mut self.sequence_receipt)?;
                self.phase = RestartUpdatePhase::PushGlobalLeaf;
            }
            RestartUpdatePhase::PushGlobalLeaf => {
                let progress = self
                    .new_global_builder
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "new global sequence builder disappeared",
                    ))?
                    .poll_push(session, &mut self.sequence_receipt)?;
                if progress == ResumableSequenceProgress::Complete {
                    self.new_global_index += 1;
                    self.phase = RestartUpdatePhase::AllocateGlobalLeaf;
                }
            }
            RestartUpdatePhase::FinishGlobal => {
                let builder =
                    self.new_global_builder
                        .as_mut()
                        .ok_or(RestartIndexError::Invalid(
                            "new global finalizer disappeared",
                        ))?;
                if builder.poll_finish(session, &mut self.sequence_receipt)?
                    == ResumableSequenceProgress::Complete
                {
                    self.new_global_replacement = Some(builder.take_root()?);
                    self.phase = RestartUpdatePhase::RetainOldGlobal;
                }
            }
            RestartUpdatePhase::RetainOldGlobal => {
                let working = self
                    .old_manifest
                    .global_root
                    .map(|root| session.retain(root))
                    .transpose()?;
                let mut splice = ResumableSequenceSplice::<GlobalOccurrenceSpec>::try_from_owned(
                    session,
                    working,
                    self.old_range.clone(),
                    self.new_global_replacement.take(),
                    &mut self.sequence_receipt,
                )?;
                if splice.poll(session, &mut self.sequence_receipt)?
                    == ResumableSequenceSplitProgress::Complete
                {
                    self.global_splice = Some(splice);
                    self.phase = RestartUpdatePhase::TakeGlobal;
                } else {
                    self.global_splice = Some(splice);
                    self.phase = RestartUpdatePhase::PollGlobalSplice;
                }
            }
            RestartUpdatePhase::PollGlobalSplice => {
                let splice = self
                    .global_splice
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "global occurrence splice disappeared",
                    ))?;
                if splice.poll(session, &mut self.sequence_receipt)?
                    == ResumableSequenceSplitProgress::Complete
                {
                    self.phase = RestartUpdatePhase::TakeGlobal;
                }
            }
            RestartUpdatePhase::TakeGlobal => {
                self.global_root = self
                    .global_splice
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "completed global occurrence splice disappeared",
                    ))?
                    .take_root()?;
                self.receipt.global_leaves_reused = self.sequence_receipt.leaves_reused;
                self.phase = RestartUpdatePhase::RetainOldDirectory;
            }
            RestartUpdatePhase::RetainOldDirectory => {
                self.working_directory = self
                    .old_manifest
                    .directory_root
                    .map(|root| session.retain(root))
                    .transpose()?;
                self.phase = RestartUpdatePhase::StartOldDeletion;
            }
            RestartUpdatePhase::StartOldDeletion => {
                if self.old_cursor == self.old_range.end {
                    self.phase = RestartUpdatePhase::StartNewInsertion;
                    return Ok(());
                }
                let occurrence = global_occurrence_at_counted(
                    session.arena(),
                    self.old_manifest.global_root,
                    self.old_cursor,
                    &mut self.query_receipt,
                )?
                .1;
                self.active_label = Some(ActiveLabelMutation::try_new(
                    session,
                    self.checkpoint.prefix_directory,
                    self.working_directory.take(),
                    LabelMutationKind::Delete {
                        expected: occurrence,
                    },
                    &mut self.query_receipt,
                )?);
                self.phase = RestartUpdatePhase::PollLabelMutation;
            }
            RestartUpdatePhase::StartNewInsertion => {
                if self.new_reverse_cursor == 0 {
                    self.phase = RestartUpdatePhase::AllocateCandidateGreen;
                    return Ok(());
                }
                let index = self.new_reverse_cursor - 1;
                let occurrence = self.new_occurrences[index];
                let descriptor = self.new_global_descriptor_ids[index];
                self.active_label = Some(ActiveLabelMutation::try_new(
                    session,
                    self.checkpoint.prefix_directory,
                    self.working_directory.take(),
                    LabelMutationKind::Insert {
                        occurrence,
                        descriptor,
                    },
                    &mut self.query_receipt,
                )?);
                self.phase = RestartUpdatePhase::PollLabelMutation;
            }
            RestartUpdatePhase::PollLabelMutation => {
                let mutation = self
                    .active_label
                    .as_mut()
                    .ok_or(RestartIndexError::Invalid(
                        "active label mutation disappeared",
                    ))?;
                if mutation.poll(session, &mut self.sequence_receipt)? {
                    self.working_directory = mutation.take_directory()?;
                    self.active_label = None;
                    if self.old_cursor < self.old_range.end {
                        self.old_cursor += 1;
                        self.receipt.old_changed_occurrences_read += 1;
                        self.phase = RestartUpdatePhase::StartOldDeletion;
                    } else {
                        self.new_reverse_cursor -= 1;
                        self.receipt.new_changed_occurrences_read += 1;
                        self.phase = RestartUpdatePhase::StartNewInsertion;
                    }
                }
            }
            RestartUpdatePhase::AllocateCandidateGreen => {
                let generation = decode_green(session.arena(), self.old_manifest.green_root)?
                    .1
                    .checked_add(1)
                    .ok_or(RestartIndexError::Invalid("green generation overflow"))?;
                let (green, _) = session.allocate_packed(
                    &encode_green(
                        self.new_source_revision,
                        generation,
                        self.candidate_source.authority_nonce,
                    ),
                    &[],
                )?;
                self.candidate_green = Some(green);
                self.phase = RestartUpdatePhase::AllocateCandidateCheckpoint;
            }
            RestartUpdatePhase::AllocateCandidateCheckpoint => {
                let candidate_green = session.owner_id(self.candidate_green.as_ref().ok_or(
                    RestartIndexError::Invalid("candidate green root disappeared"),
                )?)?;
                let mut children = Vec::with_capacity(2);
                if let Some(prefix) = self.checkpoint.prefix_directory {
                    children.push(prefix);
                }
                children.push(candidate_green);
                let payload = encode_checkpoint_manifest(
                    self.new_source_revision,
                    self.checkpoint.occurrence_high_water,
                    self.old_manifest.interner_generation,
                    self.old_manifest.source_lineage_generation,
                    self.checkpoint.prefix_directory.is_some(),
                );
                let (checkpoint, _) = session.allocate_packed(&payload, &children)?;
                self.candidate_checkpoint_id = Some(session.owner_id(&checkpoint)?);
                self.candidate_checkpoint_owner = Some(checkpoint);
                self.phase = RestartUpdatePhase::RetainInterner;
            }
            RestartUpdatePhase::RetainInterner => {
                self.interner_owner = Some(session.retain(self.old_manifest.interner_root)?);
                self.phase = RestartUpdatePhase::RetainSourceLineage;
            }
            RestartUpdatePhase::RetainSourceLineage => {
                self.source_lineage_owner =
                    Some(session.retain(self.old_manifest.source_lineage_root)?);
                self.phase = RestartUpdatePhase::AllocateOccurrenceAllocator;
            }
            RestartUpdatePhase::AllocateOccurrenceAllocator => {
                let generation = self
                    .old_manifest
                    .occurrence_allocator_generation
                    .checked_add(1)
                    .ok_or(RestartIndexError::Invalid(
                        "candidate occurrence allocator generation overflow",
                    ))?;
                let replacement_count = u64::try_from(self.new_occurrences.len())
                    .map_err(|_| RestartIndexError::Invalid("replacement length overflow"))?;
                let high_water = self
                    .old_manifest
                    .occurrence_allocator_high_water
                    .checked_add(replacement_count)
                    .ok_or(RestartIndexError::Invalid(
                        "candidate occurrence allocator high-water overflow",
                    ))?;
                let (allocator, _) = session
                    .allocate_packed(&encode_occurrence_allocator(generation, high_water), &[])?;
                self.occurrence_allocator_owner = Some(allocator);
                self.phase = RestartUpdatePhase::AllocateDonorRange;
            }
            RestartUpdatePhase::AllocateDonorRange => {
                let donor = DonorRangeManifest {
                    donor_document_root: session.arena().local_id(self.donor_document_root)?,
                    donor_global_root: self.old_manifest.global_root,
                    donor_directory_root: self.old_manifest.directory_root,
                    donor_checkpoint_root: self.old_manifest.checkpoint_root,
                    donor_interner_root: self.old_manifest.interner_root,
                    donor_green_root: self.old_manifest.green_root,
                    donor_occurrence_allocator_root: self.old_manifest.occurrence_allocator_root,
                    donor_source_revision: self.old_manifest.source_revision,
                    old_range: self.old_range.clone(),
                    donor_occurrence_count: self.old_manifest.occurrence_count,
                    interner_generation: self.old_manifest.interner_generation,
                    source_lineage_generation: self.old_manifest.source_lineage_generation,
                    source_lineage_root: self.old_manifest.source_lineage_root,
                    occurrence_allocator_generation: self
                        .old_manifest
                        .occurrence_allocator_generation,
                    occurrence_allocator_high_water: self
                        .old_manifest
                        .occurrence_allocator_high_water,
                };
                let children = [
                    donor.donor_checkpoint_root,
                    donor.donor_interner_root,
                    donor.source_lineage_root,
                    donor.donor_green_root,
                    donor.donor_occurrence_allocator_root,
                ];
                let (owner, _) = session.allocate_packed(&encode_donor_range(&donor), &children)?;
                self.donor_range_owner = Some(owner);
                self.phase = RestartUpdatePhase::AllocateSuffixAdoption;
            }
            RestartUpdatePhase::AllocateSuffixAdoption => {
                let green_id = session.owner_id(self.candidate_green.as_ref().ok_or(
                    RestartIndexError::Invalid("candidate green root disappeared"),
                )?)?;
                let payload = encode_suffix_adoption(
                    self.old_manifest.source_revision,
                    self.new_source_revision,
                    self.old_range.end..self.old_manifest.occurrence_count,
                    self.old_range.start
                        + u64::try_from(self.new_occurrences.len()).map_err(|_| {
                            RestartIndexError::Invalid("replacement length overflow")
                        })?,
                    self.old_manifest.source_lineage_generation,
                );
                let children =
                    [
                        session.owner_id(self.donor_range_owner.as_ref().ok_or(
                            RestartIndexError::Invalid("donor range owner disappeared"),
                        )?)?,
                        self.old_manifest.source_lineage_root,
                        self.checkpoint.green_root,
                        green_id,
                    ];
                let (adoption, _) = session.allocate_packed(&payload, &children)?;
                self.adoption_owner = Some(adoption);
                self.phase = RestartUpdatePhase::AllocateManifest;
            }
            RestartUpdatePhase::AllocateManifest => {
                let replacement_count = u64::try_from(self.new_occurrences.len())
                    .map_err(|_| RestartIndexError::Invalid("replacement length overflow"))?;
                let deleted = self.old_range.end - self.old_range.start;
                let occurrence_count = self
                    .old_manifest
                    .occurrence_count
                    .checked_sub(deleted)
                    .and_then(|count| count.checked_add(replacement_count))
                    .ok_or(RestartIndexError::Invalid(
                        "candidate occurrence count overflow",
                    ))?;
                let mut children = Vec::with_capacity(DOCUMENT_BASE_CHILDREN + 1);
                if let Some(global) = self.global_root.as_ref() {
                    children.push(session.owner_id(global)?);
                }
                if let Some(directory) = self.working_directory.as_ref() {
                    children.push(session.owner_id(directory)?);
                }
                children.extend([
                    session.owner_id(self.candidate_checkpoint_owner.as_ref().ok_or(
                        RestartIndexError::Invalid("candidate checkpoint owner disappeared"),
                    )?)?,
                    session.owner_id(
                        self.interner_owner
                            .as_ref()
                            .ok_or(RestartIndexError::Invalid("interner owner disappeared"))?,
                    )?,
                    session.owner_id(self.source_lineage_owner.as_ref().ok_or(
                        RestartIndexError::Invalid("source lineage owner disappeared"),
                    )?)?,
                    session.owner_id(self.candidate_green.as_ref().ok_or(
                        RestartIndexError::Invalid("candidate green owner disappeared"),
                    )?)?,
                    session.owner_id(self.occurrence_allocator_owner.as_ref().ok_or(
                        RestartIndexError::Invalid("occurrence allocator owner disappeared"),
                    )?)?,
                    session.owner_id(self.adoption_owner.as_ref().ok_or(
                        RestartIndexError::Invalid("suffix adoption owner disappeared"),
                    )?)?,
                ]);
                let payload = encode_document_manifest(
                    self.new_source_revision,
                    self.old_manifest.source_revision,
                    occurrence_count,
                    self.old_range.start,
                    self.old_range.end,
                    replacement_count,
                    self.old_manifest.interner_generation,
                    self.old_manifest.source_lineage_generation,
                    self.old_manifest
                        .occurrence_allocator_generation
                        .checked_add(1)
                        .ok_or(RestartIndexError::Invalid(
                            "candidate occurrence allocator generation overflow",
                        ))?,
                    self.old_manifest
                        .occurrence_allocator_high_water
                        .checked_add(replacement_count)
                        .ok_or(RestartIndexError::Invalid(
                            "candidate occurrence allocator high-water overflow",
                        ))?,
                    None,
                    self.global_root.is_some(),
                    self.working_directory.is_some(),
                    true,
                )?;
                let (manifest, _) = session.allocate_packed(&payload, &children)?;
                self.receipt.manifest_children_joined = children.len();
                self.terminal_owner = Some(manifest);
                self.phase = RestartUpdatePhase::ReleaseGlobal;
            }
            RestartUpdatePhase::ReleaseGlobal => {
                if let Some(owner) = self.global_root.take() {
                    session.release(owner)?;
                }
                self.phase = RestartUpdatePhase::ReleaseDirectory;
            }
            RestartUpdatePhase::ReleaseDirectory => {
                if let Some(owner) = self.working_directory.take() {
                    session.release(owner)?;
                }
                self.phase = RestartUpdatePhase::ReleaseCandidateCheckpoint;
            }
            RestartUpdatePhase::ReleaseCandidateCheckpoint => {
                session.release(self.candidate_checkpoint_owner.take().ok_or(
                    RestartIndexError::Invalid("candidate checkpoint release owner disappeared"),
                )?)?;
                self.phase = RestartUpdatePhase::ReleaseInterner;
            }
            RestartUpdatePhase::ReleaseInterner => {
                session.release(self.interner_owner.take().ok_or(
                    RestartIndexError::Invalid("interner release owner disappeared"),
                )?)?;
                self.phase = RestartUpdatePhase::ReleaseSourceLineage;
            }
            RestartUpdatePhase::ReleaseSourceLineage => {
                session.release(self.source_lineage_owner.take().ok_or(
                    RestartIndexError::Invalid("lineage release owner disappeared"),
                )?)?;
                self.phase = RestartUpdatePhase::ReleaseGreen;
            }
            RestartUpdatePhase::ReleaseGreen => {
                session.release(self.candidate_green.take().ok_or(
                    RestartIndexError::Invalid("green release owner disappeared"),
                )?)?;
                self.phase = RestartUpdatePhase::ReleaseOccurrenceAllocator;
            }
            RestartUpdatePhase::ReleaseOccurrenceAllocator => {
                session.release(self.occurrence_allocator_owner.take().ok_or(
                    RestartIndexError::Invalid("allocator release owner disappeared"),
                )?)?;
                self.phase = RestartUpdatePhase::ReleaseDonorRange;
            }
            RestartUpdatePhase::ReleaseDonorRange => {
                session.release(self.donor_range_owner.take().ok_or(
                    RestartIndexError::Invalid("donor range release owner disappeared"),
                )?)?;
                self.phase = RestartUpdatePhase::ReleaseAdoption;
            }
            RestartUpdatePhase::ReleaseAdoption => {
                session.release(self.adoption_owner.take().ok_or(
                    RestartIndexError::Invalid("adoption release owner disappeared"),
                )?)?;
                if session.live_owners()? != 1 {
                    return Err(RestartIndexError::Invalid(
                        "candidate did not reduce to one atomic restart manifest",
                    ));
                }
                self.phase = RestartUpdatePhase::Committable;
            }
            RestartUpdatePhase::Committable | RestartUpdatePhase::Failed => {
                return Err(RestartIndexError::Invalid(
                    "restart update has no pollable phase",
                ));
            }
        }
        Ok(())
    }

    fn commit(
        mut self,
        session: ArenaBuildSession<'_>,
    ) -> Result<RestartIndexDocument, RestartIndexError> {
        if self.phase != RestartUpdatePhase::Committable || session.id() != self.build {
            return Err(RestartIndexError::Invalid(
                "restart update is not committable by this session",
            ));
        }
        let terminal = self
            .terminal_owner
            .take()
            .ok_or(RestartIndexError::Invalid(
                "committable restart manifest disappeared",
            ))?;
        let checkpoint = session
            .arena()
            .scoped_query_id(
                self.candidate_checkpoint_id
                    .ok_or(RestartIndexError::Invalid(
                        "committable candidate checkpoint identity disappeared",
                    ))?,
            )?;
        let owner = session.commit(terminal)?;
        Ok(RestartIndexDocument {
            owner: Some(owner),
            checkpoint,
            initial_receipt: InitialBuildReceipt::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistent_blob::{
        PersistentBlobBuildProgress, PersistentBlobReadProgress, PersistentByteBlobBuilder,
        PersistentByteBlobReadCursor,
    };
    use crate::reference_label_interner::{
        CandidateReferenceLabel, ReferenceLabelInterner, ReferenceLabelInternerProgress,
    };
    use crate::source::SourceStore;

    #[derive(Debug)]
    struct FixtureParserAuthority {
        source_revision: u64,
    }

    impl FixtureParserAuthority {
        const fn new(source_revision: u64) -> Self {
            Self { source_revision }
        }

        fn occurrence(
            &self,
            occurrence_id: u64,
            label_id: u64,
            destination_id: u64,
        ) -> ReferenceOccurrence {
            ReferenceOccurrence {
                occurrence_id,
                label_id,
                value: ReferenceOccurrenceValue::FixtureLegacy {
                    destination_id,
                    coordinate_generation: self.source_revision,
                    coordinate: FixtureRetainedOccurrenceCoordinate {
                        source_piece: 10_000 + occurrence_id,
                        source_piece_offset: occurrence_id * 3,
                        source_length: 7,
                        projection_program: 20_000 + occurrence_id,
                        projection_logical_offset: occurrence_id * 2,
                    },
                },
            }
        }
    }

    fn reclaim_all(arena: &mut PageArena) {
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(10_000).expect("restart test reclaim");
        }
    }

    fn build_cooked_blob(session: &mut ArenaBuildSession<'_>, bytes: &[u8]) -> PersistentByteBlob {
        let mut builder = PersistentByteBlobBuilder::try_new(session.id()).unwrap();
        let mut offset = 0;
        while offset < bytes.len() {
            offset += builder.push_bytes(&bytes[offset..]).unwrap();
            while !builder.is_ready_for_bytes() {
                assert_ne!(
                    builder.poll(session).unwrap(),
                    PersistentBlobBuildProgress::Complete
                );
            }
        }
        builder.begin_finish().unwrap();
        while builder.poll(session).unwrap() != PersistentBlobBuildProgress::Complete {}
        builder.take_blob().unwrap()
    }

    fn intern_candidate_label(
        interner: &mut ReferenceLabelInterner,
        session: &mut ArenaBuildSession<'_>,
        label: &str,
        nonce: u64,
    ) -> InternedReferenceLabel {
        interner
            .begin_intern(CandidateReferenceLabel::proof_only(label, nonce))
            .unwrap();
        while interner.poll(session).unwrap() != ReferenceLabelInternerProgress::LabelReady {}
        interner.take_label().unwrap()
    }

    fn append_cooked_occurrence(
        builder: &mut ReferenceCandidateIndexBuilder,
        interner: &mut ReferenceLabelInterner,
        session: &mut ArenaBuildSession<'_>,
        label: &str,
        destination: &[u8],
        title: Option<&[u8]>,
        nonce: u64,
    ) {
        let label = intern_candidate_label(interner, session, label, nonce);
        let destination = build_cooked_blob(session, destination);
        let title = title.map(|value| build_cooked_blob(session, value));
        builder
            .begin_occurrence(
                session,
                WriterAuthenticatedReferenceOccurrence::proof_only(label, destination, title),
            )
            .unwrap();
        while builder.poll(session).unwrap() != ReferenceCandidateIndexProgress::OccurrenceAckReady
        {
        }
        interner
            .acknowledge_label_use(builder.take_occurrence_ack().unwrap().into_label_use())
            .unwrap();
    }

    fn finish_candidate_index(
        builder: &mut ReferenceCandidateIndexBuilder,
        interner: &mut ReferenceLabelInterner,
        session: &mut ArenaBuildSession<'_>,
    ) -> ReferenceCandidateIndexManifest {
        interner.begin_finish().unwrap();
        while interner.poll(session).unwrap() != ReferenceLabelInternerProgress::ManifestReady {}
        builder
            .begin_finish(session, interner.take_manifest().unwrap())
            .unwrap();
        while builder.poll(session).unwrap() != ReferenceCandidateIndexProgress::ManifestReady {}
        builder.take_manifest().unwrap()
    }

    fn committed_reference_index(
        arena: &PageArena,
        document: &RestartIndexDocument,
    ) -> CommittedReferenceIndex {
        CommittedReferenceIndex::proof_only(arena, document.root().unwrap(), document.checkpoint())
            .unwrap()
    }

    fn append_restart_cooked_occurrence(
        builder: &mut ReferenceRestartIndexBuilder,
        interner: &mut ReferenceLabelInterner,
        session: &mut ArenaBuildSession<'_>,
        label: &str,
        destination: &[u8],
        title: Option<&[u8]>,
        nonce: u64,
    ) {
        let label = intern_candidate_label(interner, session, label, nonce);
        let destination = build_cooked_blob(session, destination);
        let title = title.map(|value| build_cooked_blob(session, value));
        builder
            .begin_occurrence(
                session,
                WriterAuthenticatedReferenceOccurrence::proof_only(label, destination, title),
            )
            .unwrap();
        while builder.poll(session).unwrap() != ReferenceRestartIndexProgress::OccurrenceAckReady {}
        interner
            .acknowledge_label_use(builder.take_occurrence_ack().unwrap().into_label_use())
            .unwrap();
    }

    fn committed_winner(
        arena: &PageArena,
        document: CommittedReferenceIndex,
        raw_label: &str,
    ) -> Option<CommittedReferenceWinner> {
        let mut query = document.begin_winner_query(arena, raw_label).unwrap();
        for _ in 0..100_000 {
            if query.poll(arena).unwrap() == CommittedReferenceWinnerQueryProgress::Ready {
                assert!(query.receipt().maximum_semantic_nodes_per_poll <= 512);
                return query.take().unwrap();
            }
        }
        panic!("committed winner query did not complete")
    }

    fn read_committed_blob(arena: &PageArena, view: CommittedReferenceBlobView) -> Vec<u8> {
        let mut cursor = PersistentByteBlobReadCursor::try_new(view.metadata()).unwrap();
        let mut output = Vec::new();
        loop {
            match cursor.poll(arena).unwrap() {
                PersistentBlobReadProgress::Pending => {}
                PersistentBlobReadProgress::Chunk(chunk) => {
                    output.extend_from_slice(chunk.bytes(arena).unwrap());
                }
                PersistentBlobReadProgress::Complete => return output,
            }
        }
    }

    fn candidate_builder_for_source(
        session: &ArenaBuildSession<'_>,
        source: SourceSnapshotDescriptor,
    ) -> ReferenceCandidateIndexBuilder {
        ReferenceCandidateIndexBuilder::new(ReferenceCandidateIndexAuthority::proof_only(
            session.id(),
            source,
            11,
            1,
            77,
        ))
        .unwrap()
    }

    fn new_candidate_builder(session: &ArenaBuildSession<'_>) -> ReferenceCandidateIndexBuilder {
        candidate_builder_for_source(
            session,
            SourceSnapshotDescriptor {
                revision: SourceRevision(1),
                root: SourceRootId(101),
                bytes: 1024,
            },
        )
    }

    fn decode_crossed_source_manifest(
        document_source: SourceSnapshotDescriptor,
        lineage_source: SourceSnapshotDescriptor,
    ) -> RestartIndexError {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();

        let (interner, _) = session
            .allocate_packed(&encode_interner_authority(9, 0, false), &[])
            .unwrap();
        let interner_id = session.owner_id(&interner).unwrap();
        let (source_lineage, _) = session
            .allocate_packed(
                &encode_source_lineage_authority(11, lineage_source).unwrap(),
                &[],
            )
            .unwrap();
        let source_lineage_id = session.owner_id(&source_lineage).unwrap();
        let (green, _) = session
            .allocate_packed(&encode_green(document_source.revision.0, 1, 77), &[])
            .unwrap();
        let green_id = session.owner_id(&green).unwrap();
        let (allocator, _) = session
            .allocate_packed(&encode_occurrence_allocator(1, 0), &[])
            .unwrap();
        let allocator_id = session.owner_id(&allocator).unwrap();
        let (checkpoint, _) = session
            .allocate_packed(
                &encode_checkpoint_manifest(document_source.revision.0, 0, 9, 11, false),
                &[green_id],
            )
            .unwrap();
        let checkpoint_id = session.owner_id(&checkpoint).unwrap();
        let payload = encode_document_manifest(
            document_source.revision.0,
            document_source.revision.0,
            0,
            0,
            0,
            0,
            9,
            11,
            1,
            0,
            Some(document_source),
            false,
            false,
            false,
        )
        .unwrap();
        let (document, _) = session
            .allocate_packed(
                &payload,
                &[
                    checkpoint_id,
                    interner_id,
                    source_lineage_id,
                    green_id,
                    allocator_id,
                ],
            )
            .unwrap();
        let document_id = session.owner_id(&document).unwrap();
        session.release(checkpoint).unwrap();
        session.release(interner).unwrap();
        session.release(source_lineage).unwrap();
        session.release(green).unwrap();
        session.release(allocator).unwrap();
        let owner = session.commit(document).unwrap();
        let error = decode_document_manifest(&arena, document_id).unwrap_err();
        arena.release_later(owner).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        error
    }

    #[derive(Debug, PartialEq, Eq)]
    enum CandidateAbortProbe {
        Complete(u64),
        Fault(RestartIndexError),
        Cancelled(u64),
    }

    fn run_candidate_abort_probe(
        arena: &mut PageArena,
        fault_after_task: Option<u64>,
        cancel_after_task: Option<u64>,
    ) -> CandidateAbortProbe {
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut interner = ReferenceLabelInterner::new_initial(session.id(), 9).unwrap();
        let mut builder = new_candidate_builder(&session);
        if let Some(task) = fault_after_task {
            builder = builder.with_fault_after_task(task);
        }

        macro_rules! abort_with {
            ($outcome:expr) => {{
                let abort = session.begin_abort().unwrap();
                drop(builder);
                drop(interner);
                while !arena.poll_build_abort(abort, 1).unwrap().complete {}
                reclaim_all(arena);
                return $outcome;
            }};
        }

        for index in 0..2_u64 {
            let label = intern_candidate_label(&mut interner, &mut session, "same", index + 1);
            let destination = build_cooked_blob(&mut session, b"cooked-value");
            builder
                .begin_occurrence(
                    &session,
                    WriterAuthenticatedReferenceOccurrence::proof_only(label, destination, None),
                )
                .unwrap();
            if cancel_after_task == Some(builder.receipt().bounded_tasks()) {
                let tasks = builder.receipt().bounded_tasks();
                abort_with!(CandidateAbortProbe::Cancelled(tasks));
            }
            loop {
                let progress = match builder.poll(&mut session) {
                    Ok(progress) => progress,
                    Err(error) => abort_with!(CandidateAbortProbe::Fault(error)),
                };
                let tasks = builder.receipt().bounded_tasks();
                if cancel_after_task == Some(tasks) {
                    abort_with!(CandidateAbortProbe::Cancelled(tasks));
                }
                if progress == ReferenceCandidateIndexProgress::OccurrenceAckReady {
                    interner
                        .acknowledge_label_use(
                            builder.take_occurrence_ack().unwrap().into_label_use(),
                        )
                        .unwrap();
                    break;
                }
            }
            if index == 0 {
                builder.capture_checkpoint(&mut session).unwrap();
            }
        }

        interner.begin_finish().unwrap();
        while interner.poll(&mut session).unwrap() != ReferenceLabelInternerProgress::ManifestReady
        {
        }
        builder
            .begin_finish(&session, interner.take_manifest().unwrap())
            .unwrap();
        loop {
            let progress = match builder.poll(&mut session) {
                Ok(progress) => progress,
                Err(error) => abort_with!(CandidateAbortProbe::Fault(error)),
            };
            let tasks = builder.receipt().bounded_tasks();
            if cancel_after_task == Some(tasks) {
                abort_with!(CandidateAbortProbe::Cancelled(tasks));
            }
            if progress == ReferenceCandidateIndexProgress::ManifestReady {
                abort_with!(CandidateAbortProbe::Complete(tasks));
            }
        }
    }

    fn run_update(
        arena: &mut PageArena,
        old: &RestartIndexDocument,
        range: Range<u64>,
        change_set: CandidateChangeSet,
        fault_after: Option<u64>,
    ) -> Result<(RestartIndexDocument, RestartUpdateReceipt), RestartIndexError> {
        let convergence =
            AuthenticatedConvergenceRange::try_mint(arena, old, range.start, range.end)?;
        let mut ticket = arena.begin_build()?;
        let mut job = RestartIndexUpdateJob::try_new(&ticket, arena, old, convergence, change_set)?;
        if let Some(task) = fault_after {
            job = job.with_fault_after_task(task);
        }
        loop {
            let mut session = arena
                .resume_build(ticket)
                .map_err(|failure| RestartIndexError::ArenaBuild(failure.error))?;
            match job.poll(&mut session) {
                Ok(RestartUpdateProgress::Pending) => {
                    ticket = session.suspend()?;
                }
                Ok(RestartUpdateProgress::Committable) => {
                    let receipt = job.receipt();
                    let document = job.commit(session)?;
                    return Ok((document, receipt));
                }
                Err(error) => {
                    let abort = session.begin_abort()?;
                    while !arena.poll_build_abort(abort, 1)?.complete {}
                    drop(job);
                    reclaim_all(arena);
                    return Err(error);
                }
            }
        }
    }

    fn candidate_change_set(
        arena: &PageArena,
        old: &RestartIndexDocument,
        revision: u64,
        values: &[(u64, u64)],
    ) -> CandidateChangeSet {
        let mut authority =
            RestartWriterAuthority::from_document(arena, old, revision, 70 + revision).unwrap();
        let mut drafts = Vec::with_capacity(values.len());
        for (index, &(label, destination)) in values.iter().enumerate() {
            let coordinate = u64::try_from(index + 1).unwrap();
            drafts.push(
                authority
                    .occurrence(
                        label,
                        destination,
                        30_000 + coordinate,
                        coordinate * 5,
                        11,
                        40_000 + coordinate,
                        coordinate * 7,
                    )
                    .unwrap(),
            );
        }
        authority.finish(drafts)
    }

    fn ids(values: &[ReferenceOccurrence]) -> Vec<u64> {
        values.iter().map(|value| value.occurrence_id).collect()
    }

    fn cancel_update_after_tasks(
        arena: &mut PageArena,
        old: &RestartIndexDocument,
        range: Range<u64>,
        change_set: CandidateChangeSet,
        tasks: u64,
    ) {
        let convergence =
            AuthenticatedConvergenceRange::try_mint(arena, old, range.start, range.end).unwrap();
        let mut ticket = arena.begin_build().unwrap();
        let mut job =
            RestartIndexUpdateJob::try_new(&ticket, arena, old, convergence, change_set).unwrap();
        for _ in 0..tasks {
            let mut session = arena.resume_build(ticket).unwrap();
            job.poll(&mut session).unwrap();
            ticket = session.suspend().unwrap();
        }
        let abort = arena.begin_build_abort(ticket).unwrap();
        drop(job);
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        reclaim_all(arena);
    }

    #[test]
    fn contiguous_restart_rewins_without_page_order_or_suffix_enumeration() {
        let mut arena = PageArena::new();
        let old_authority = FixtureParserAuthority::new(1);
        let old_values = [
            (1, 10, 100), // acknowledged prefix winner for label 10
            (2, 20, 200), // suffix-only label's old winner
            (3, 10, 101), // changed duplicate
            (4, 30, 300), // deleted last definition
            (5, 40, 400), // relabelled to 50
            (6, 20, 201), // untouched duplicate promoted behind insertion
            (7, 10, 102), // untouched duplicate
            (8, 60, 600), // wholly untouched suffix-only label
        ];
        let old_occurrences = old_values
            .iter()
            .map(|&(occurrence, label, destination)| {
                old_authority.occurrence(occurrence, label, destination)
            })
            .collect();
        let old = build_initial_document(
            &mut arena,
            old_occurrences,
            ReferenceCheckpointSeal::try_new(1, false, false).unwrap(),
            1,
            9,
            11,
        )
        .unwrap();

        let old_suffix_global = (5..8)
            .map(|index| old.occurrence_at(&arena, index).unwrap().0)
            .collect::<Vec<_>>();
        let old_label_20_suffix = {
            let manifest = old.manifest(&arena).unwrap();
            let lookup = lookup_directory(&arena, manifest.directory_root, 20).unwrap();
            per_label_occurrence_at(&arena, lookup.sequence_root, 1)
                .unwrap()
                .0
        };

        let change_set = candidate_change_set(
            &arena,
            &old,
            2,
            &[(10, 999), (50, 400), (20, 222), (10, 103)],
        );
        let (next, receipt) = run_update(&mut arena, &old, 1..5, change_set, None).unwrap();

        assert_eq!(receipt.old_changed_occurrences_read, 4);
        assert_eq!(receipt.new_changed_occurrences_read, 4);
        assert_eq!(receipt.suffix_occurrences_enumerated, 0);
        assert!(receipt.global_leaves_reused >= 4);
        assert!(receipt.maximum_branches_per_task <= 1);
        assert_eq!(receipt.manifest_children_joined, 8);
        assert!(receipt.maximum_query_sequence_nodes_per_task <= MAX_QUERY_SEQUENCE_NODES_PER_TASK);

        assert_eq!(next.winner(&arena, 10).unwrap().unwrap().2.occurrence_id, 1);
        assert_eq!(
            next.winner(&arena, 20).unwrap().unwrap().2.occurrence_id,
            11
        );
        assert_eq!(
            next.winner(&arena, 50).unwrap().unwrap().2.occurrence_id,
            10
        );
        assert!(next.winner(&arena, 30).unwrap().is_none());
        assert!(next.winner(&arena, 40).unwrap().is_none());
        assert_eq!(next.winner(&arena, 60).unwrap().unwrap().2.occurrence_id, 8);
        assert_eq!(
            ids(&next.label_occurrences(&arena, 10).unwrap()),
            [1, 9, 12, 7]
        );
        assert_eq!(ids(&next.label_occurrences(&arena, 20).unwrap()), [11, 6]);

        let expected_global_ids = [1, 9, 10, 11, 12, 6, 7, 8];
        for (index, expected) in expected_global_ids.into_iter().enumerate() {
            assert_eq!(
                next.occurrence_at(&arena, u64::try_from(index).unwrap())
                    .unwrap()
                    .1
                    .occurrence_id,
                expected
            );
        }
        for (offset, old_leaf) in old_suffix_global.into_iter().enumerate() {
            assert_eq!(
                next.occurrence_at(&arena, u64::try_from(5 + offset).unwrap())
                    .unwrap()
                    .0,
                old_leaf,
                "untouched suffix descriptor identity changed"
            );
        }
        let next_manifest = next.manifest(&arena).unwrap();
        let next_label_20 = lookup_directory(&arena, next_manifest.directory_root, 20).unwrap();
        assert_eq!(
            per_label_occurrence_at(&arena, next_label_20.sequence_root, 1)
                .unwrap()
                .0,
            old_label_20_suffix,
            "per-label suffix wrapper identity changed"
        );
        let adoption = decode_suffix_adoption(
            &arena,
            next_manifest
                .adoption_root
                .expect("candidate adoption root"),
        )
        .unwrap();
        assert_eq!((adoption.old_suffix_start, adoption.old_suffix_end), (5, 8));
        assert_eq!(adoption.new_suffix_start, 5);

        next.release_later(&mut arena).unwrap();
        old.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn chained_second_and_third_edits_remint_checkpoint_and_allocator_authority() {
        let mut arena = PageArena::new();
        let authority = FixtureParserAuthority::new(1);
        let initial = build_initial_document(
            &mut arena,
            vec![
                authority.occurrence(1, 10, 100),
                authority.occurrence(2, 20, 200),
                authority.occurrence(3, 10, 101),
                authority.occurrence(4, 30, 300),
                authority.occurrence(5, 20, 201),
            ],
            ReferenceCheckpointSeal::try_new(1, false, false).unwrap(),
            1,
            9,
            11,
        )
        .unwrap();

        let first_change = candidate_change_set(&arena, &initial, 2, &[(40, 400), (20, 222)]);
        let (first, first_receipt) =
            run_update(&mut arena, &initial, 1..3, first_change, None).unwrap();
        assert_eq!(first_receipt.suffix_occurrences_enumerated, 0);
        assert_ne!(first.checkpoint(), initial.checkpoint());
        let first_manifest = first.manifest(&arena).unwrap();
        assert_eq!(first_manifest.parent_source_revision, 1);
        assert_eq!(first_manifest.source_revision, 2);
        assert_eq!(first_manifest.occurrence_allocator_generation, 2);
        assert_eq!(first_manifest.occurrence_allocator_high_water, 7);

        // The second edit replaces both revision-2 descriptors and a retained
        // revision-1 descriptor.  Its suffix still contains revision-1 data.
        let second_change = candidate_change_set(&arena, &first, 3, &[(30, 333), (50, 500)]);
        let (second, second_receipt) =
            run_update(&mut arena, &first, 1..4, second_change, None).unwrap();
        assert_eq!(second_receipt.suffix_occurrences_enumerated, 0);
        assert_ne!(second.checkpoint(), first.checkpoint());
        let second_manifest = second.manifest(&arena).unwrap();
        assert_eq!(second_manifest.parent_source_revision, 2);
        assert_eq!(second_manifest.source_revision, 3);
        assert_eq!(second_manifest.occurrence_allocator_generation, 3);
        assert_eq!(second_manifest.occurrence_allocator_high_water, 9);

        let third_change = candidate_change_set(&arena, &second, 4, &[(20, 333)]);
        let (third, third_receipt) =
            run_update(&mut arena, &second, 1..2, third_change, None).unwrap();
        assert_eq!(third_receipt.suffix_occurrences_enumerated, 0);
        assert_ne!(third.checkpoint(), second.checkpoint());
        let third_manifest = third.manifest(&arena).unwrap();
        assert_eq!(third_manifest.parent_source_revision, 3);
        assert_eq!(third_manifest.source_revision, 4);
        assert_eq!(third_manifest.occurrence_allocator_generation, 4);
        assert_eq!(third_manifest.occurrence_allocator_high_water, 10);

        let expected_global_ids = [1, 10, 9, 5];
        for (index, expected) in expected_global_ids.into_iter().enumerate() {
            assert_eq!(
                third
                    .occurrence_at(&arena, u64::try_from(index).unwrap())
                    .unwrap()
                    .1
                    .occurrence_id,
                expected
            );
        }
        assert_eq!(
            third.winner(&arena, 10).unwrap().unwrap().2.occurrence_id,
            1
        );
        assert_eq!(
            third.winner(&arena, 20).unwrap().unwrap().2.occurrence_id,
            10
        );
        assert_eq!(ids(&third.label_occurrences(&arena, 20).unwrap()), [10, 5]);
        assert_eq!(
            third.winner(&arena, 50).unwrap().unwrap().2.occurrence_id,
            9
        );
        assert!(third.winner(&arena, 30).unwrap().is_none());
        assert!(third.winner(&arena, 40).unwrap().is_none());

        third.release_later(&mut arena).unwrap();
        second.release_later(&mut arena).unwrap();
        first.release_later(&mut arena).unwrap();
        initial.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn convergence_rejects_an_edit_before_the_checkpoint() {
        let mut arena = PageArena::new();
        let authority = FixtureParserAuthority::new(1);
        let old = build_initial_document(
            &mut arena,
            vec![
                authority.occurrence(1, 10, 100),
                authority.occurrence(2, 20, 200),
                authority.occurrence(3, 30, 300),
            ],
            ReferenceCheckpointSeal::try_new(2, false, false).unwrap(),
            1,
            9,
            11,
        )
        .unwrap();

        assert!(matches!(
            AuthenticatedConvergenceRange::try_mint(&arena, &old, 1, 3),
            Err(RestartIndexError::Invalid(
                "parser convergence is outside its authenticated checkpoint/donor range"
            ))
        ));

        old.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn checkpoint_seal_excludes_pending_definitions_and_active_paragraphs() {
        assert!(ReferenceCheckpointSeal::try_new(7, false, false).is_ok());
        assert_eq!(
            ReferenceCheckpointSeal::try_new(7, true, false),
            Err(RestartIndexError::Invalid(
                "reference checkpoint requires every definition ack and no active Paragraph"
            ))
        );
        assert_eq!(
            ReferenceCheckpointSeal::try_new(7, false, true),
            Err(RestartIndexError::Invalid(
                "reference checkpoint requires every definition ack and no active Paragraph"
            ))
        );
    }

    #[test]
    fn writer_authority_rejects_a_stale_candidate_source_revision() {
        let mut arena = PageArena::new();
        let authority = FixtureParserAuthority::new(2);
        let old = build_initial_document(
            &mut arena,
            vec![authority.occurrence(1, 10, 100)],
            ReferenceCheckpointSeal::try_new(0, false, false).unwrap(),
            2,
            9,
            11,
        )
        .unwrap();

        assert!(matches!(
            RestartWriterAuthority::from_document(&arena, &old, 1, 70),
            Err(RestartIndexError::Invalid(
                "candidate source revision did not advance"
            ))
        ));

        old.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn crossed_checkpoint_writer_lineage_and_per_label_root_are_rejected() {
        let mut arena = PageArena::new();
        let authority = FixtureParserAuthority::new(1);
        let make = |base| {
            vec![
                authority.occurrence(base, 10, 100),
                authority.occurrence(base + 1, 20, 200),
            ]
        };
        let old_a = build_initial_document(
            &mut arena,
            make(1),
            ReferenceCheckpointSeal::try_new(1, false, false).unwrap(),
            1,
            9,
            11,
        )
        .unwrap();
        let old_b = build_initial_document(
            &mut arena,
            make(1),
            ReferenceCheckpointSeal::try_new(1, false, false).unwrap(),
            1,
            9,
            11,
        )
        .unwrap();

        let crossed_convergence =
            AuthenticatedConvergenceRange::try_mint(&arena, &old_b, 1, 2).unwrap();
        let local_change_set = candidate_change_set(&arena, &old_a, 2, &[]);
        let ticket = arena.begin_build().unwrap();
        assert!(matches!(
            RestartIndexUpdateJob::try_new(
                &ticket,
                &arena,
                &old_a,
                crossed_convergence,
                local_change_set,
            ),
            Err(RestartIndexError::Invalid(
                "authenticated convergence crossed the selected donor document"
            ))
        ));
        let abort = arena.begin_build_abort(ticket).unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}

        let local_convergence =
            AuthenticatedConvergenceRange::try_mint(&arena, &old_a, 1, 2).unwrap();
        let foreign_change_set = candidate_change_set(&arena, &old_b, 2, &[(20, 999)]);
        let ticket = arena.begin_build().unwrap();
        assert!(matches!(
            RestartIndexUpdateJob::try_new(
                &ticket,
                &arena,
                &old_a,
                local_convergence,
                foreign_change_set,
            ),
            Err(RestartIndexError::Invalid(
                "candidate source seal crossed donor/interner/source/allocator authority"
            ))
        ));
        let abort = arena.begin_build_abort(ticket).unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}

        // Even if a valid local seal is retained, it cannot launder one draft
        // minted by another donor's writer authority.  Both donors deliberately
        // use the same occurrence IDs and generation scalars here, so identity
        // equality cannot accidentally make the foreign capability acceptable.
        let local_convergence =
            AuthenticatedConvergenceRange::try_mint(&arena, &old_a, 1, 2).unwrap();
        let local_authority =
            RestartWriterAuthority::from_document(&arena, &old_a, 2, 701).unwrap();
        let mut foreign_authority =
            RestartWriterAuthority::from_document(&arena, &old_b, 2, 702).unwrap();
        let foreign_draft = foreign_authority
            .occurrence(20, 999, 31_000, 5, 11, 41_000, 7)
            .unwrap();
        let crossed_draft_set = local_authority.finish(vec![foreign_draft]);
        let ticket = arena.begin_build().unwrap();
        assert!(matches!(
            RestartIndexUpdateJob::try_new(
                &ticket,
                &arena,
                &old_a,
                local_convergence,
                crossed_draft_set,
            ),
            Err(RestartIndexError::Invalid(
                "new occurrence crossed writer/interner/source/allocator authority"
            ))
        ));
        let abort = arena.begin_build_abort(ticket).unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}

        // A same-shaped leaf cannot cross a different exact label even when
        // its occurrence count is identical.
        let occurrence = authority.occurrence(500, 20, 500);
        let descriptor = arena
            .allocate_packed(&encode_global_occurrence(occurrence), &[])
            .unwrap()
            .owner;
        let wrapper = arena
            .allocate_packed(&encode_label_occurrence(occurrence), &[descriptor.id()])
            .unwrap()
            .owner;
        let crossed_directory = arena
            .allocate_packed(&encode_directory_leaf(10, 1), &[wrapper.id()])
            .unwrap()
            .owner;
        assert!(matches!(
            lookup_directory(&arena, Some(crossed_directory.id()), 10),
            Err(RestartIndexError::Invalid(
                "label-directory leaf length disagrees with its sequence"
            ))
        ));
        arena.release_later(crossed_directory).unwrap();
        arena.release_later(wrapper).unwrap();
        arena.release_later(descriptor).unwrap();

        old_b.release_later(&mut arena).unwrap();
        old_a.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn pathological_repeated_label_is_linear_and_retains_a_huge_suffix() {
        const COUNT: usize = 65_536;
        const CUT: usize = COUNT / 2;
        let mut arena = PageArena::new();
        let authority = FixtureParserAuthority::new(1);
        let occurrences = (0..COUNT)
            .map(|index| {
                let id = u64::try_from(index + 1).unwrap();
                authority.occurrence(id, 10, 100_000 + id)
            })
            .collect::<Vec<_>>();
        let old = build_initial_document(
            &mut arena,
            occurrences,
            ReferenceCheckpointSeal::try_new(u64::try_from(CUT).unwrap(), false, false).unwrap(),
            1,
            9,
            11,
        )
        .unwrap();
        let initial = old.initial_receipt();
        assert_eq!(initial.occurrences, u64::try_from(COUNT).unwrap());
        assert_eq!(initial.exact_labels, 1);
        assert_eq!(initial.selected_checkpoint_root_edges, 1);
        assert!(initial.selected_checkpoint_history_nodes_upper_bound <= 64);
        assert!(
            initial.sequence_branches_allocated < u64::try_from(2 * COUNT + 256).unwrap(),
            "initial streaming build became superlinear: {initial:?}"
        );
        assert!(
            initial.append_splice_branches_allocated < u64::try_from(COUNT + 256).unwrap(),
            "checkpoint-to-full append became O(n log n): {initial:?}"
        );

        let far_global = old
            .occurrence_at(&arena, u64::try_from(COUNT - 1).unwrap())
            .unwrap()
            .0;
        let old_manifest = old.manifest(&arena).unwrap();
        let old_label = lookup_directory(&arena, old_manifest.directory_root, 10).unwrap();
        let far_label = per_label_occurrence_at(
            &arena,
            old_label.sequence_root,
            u64::try_from(COUNT - 1).unwrap(),
        )
        .unwrap()
        .0;
        let change_set = candidate_change_set(&arena, &old, 2, &[(10, 900_002)]);
        let cut = u64::try_from(CUT).unwrap();
        let (next, update) = run_update(&mut arena, &old, cut..cut + 1, change_set, None).unwrap();
        assert_eq!(update.old_changed_occurrences_read, 1);
        assert_eq!(update.new_changed_occurrences_read, 1);
        assert_eq!(update.suffix_occurrences_enumerated, 0);
        assert!(update.global_leaves_reused >= COUNT - 1);
        assert!(update.sequence_branches_allocated < 512, "{update:?}");
        assert!(
            update.maximum_query_sequence_nodes_per_task <= MAX_QUERY_SEQUENCE_NODES_PER_TASK,
            "{update:?}"
        );
        assert_eq!(
            next.occurrence_at(&arena, u64::try_from(COUNT - 1).unwrap())
                .unwrap()
                .0,
            far_global
        );
        let next_manifest = next.manifest(&arena).unwrap();
        let next_label = lookup_directory(&arena, next_manifest.directory_root, 10).unwrap();
        assert_eq!(
            per_label_occurrence_at(
                &arena,
                next_label.sequence_root,
                u64::try_from(COUNT - 1).unwrap(),
            )
            .unwrap()
            .0,
            far_label
        );

        next.release_later(&mut arena).unwrap();
        old.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn every_outer_task_fault_and_cancel_leaves_old_manifest_atomic() {
        let mut arena = PageArena::new();
        let authority = FixtureParserAuthority::new(1);
        let occurrences = vec![
            authority.occurrence(1, 10, 100),
            authority.occurrence(2, 10, 101),
            authority.occurrence(3, 20, 200),
            authority.occurrence(4, 10, 102),
        ];
        let old = build_initial_document(
            &mut arena,
            occurrences,
            ReferenceCheckpointSeal::try_new(1, false, false).unwrap(),
            1,
            9,
            11,
        )
        .unwrap();
        let old_root = arena.local_id(old.root().unwrap()).unwrap();
        let baseline_nodes = arena.metrics().live_nodes;
        let values = [(10, 999), (20, 222)];
        let change_set = candidate_change_set(&arena, &old, 2, &values);
        let (probe, baseline) = run_update(&mut arena, &old, 1..3, change_set, None).unwrap();
        assert!(baseline.bounded_tasks > 20);
        probe.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, baseline_nodes);

        for task in 1..=baseline.bounded_tasks {
            let change_set = candidate_change_set(&arena, &old, 2, &values);
            let error = run_update(&mut arena, &old, 1..3, change_set, Some(task)).unwrap_err();
            assert_eq!(error, RestartIndexError::InjectedFault(task));
            assert!(arena.contains(old_root));
            assert_eq!(old.winner(&arena, 10).unwrap().unwrap().2.occurrence_id, 1);
            assert_eq!(arena.metrics().live_nodes, baseline_nodes);
        }

        for tasks in 0..=baseline.bounded_tasks {
            let change_set = candidate_change_set(&arena, &old, 2, &values);
            cancel_update_after_tasks(&mut arena, &old, 1..3, change_set, tasks);
            assert!(arena.contains(old_root));
            assert_eq!(old.winner(&arena, 20).unwrap().unwrap().2.occurrence_id, 3);
            assert_eq!(arena.metrics().live_nodes, baseline_nodes);
        }

        old.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn candidate_stream_accepts_real_initial_source_revision_zero() {
        let source = SourceStore::try_new("[label]: /destination\n", 8).unwrap();
        let descriptor = source.descriptor();
        assert_eq!(descriptor.revision, SourceRevision(0));
        assert_ne!(descriptor.root, SourceRootId(0));

        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut interner = ReferenceLabelInterner::new_initial(session.id(), 9).unwrap();
        let mut builder = candidate_builder_for_source(&session, descriptor);
        append_cooked_occurrence(
            &mut builder,
            &mut interner,
            &mut session,
            "label",
            b"/destination",
            None,
            1,
        );
        builder.capture_checkpoint(&mut session).unwrap();
        let manifest = finish_candidate_index(&mut builder, &mut interner, &mut session);
        let receipt = manifest.receipt();
        assert!(!receipt.restart_interner_adoption_proven());
        assert!(!receipt.committed_exact_label_lookup_proven());
        assert!(!receipt.restart_changed_interval_streaming_proven());

        let document = manifest.commit_for_test(session).unwrap();
        let decoded = document.manifest(&arena).unwrap();
        assert_eq!(decoded.source_revision, 0);
        assert_eq!(decoded.source_snapshot, Some(descriptor));
        assert_eq!(decode_green(&arena, decoded.green_root).unwrap().0, 0);
        assert_eq!(
            decode_source_lineage_authority(&arena, decoded.source_lineage_root)
                .unwrap()
                .1,
            Some((descriptor.root, descriptor.bytes))
        );
        document.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        drop(source);
    }

    #[test]
    fn production_restart_streams_one_spool_rewins_and_resolves_cooked_values() {
        let mut source = SourceStore::try_new("initial reference source\n", 8).unwrap();
        let initial_source = source.descriptor();
        assert_eq!(initial_source.revision, SourceRevision(0));

        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut interner = ReferenceLabelInterner::new_initial(session.id(), 9).unwrap();
        let mut initial = candidate_builder_for_source(&session, initial_source);
        let definitions: [(&str, &[u8]); 7] = [
            ("alpha", b"alpha-prefix"),
            ("beta", b"beta-deleted"),
            ("zeta", b"zeta-deleted-winner"),
            ("gamma", b"gamma-relabelled"),
            ("zeta", b"zeta-promoted-suffix"),
            ("beta", b"beta-suffix"),
            ("delta", b"delta-untouched-suffix"),
        ];
        for (index, (label, destination)) in definitions.into_iter().enumerate() {
            append_cooked_occurrence(
                &mut initial,
                &mut interner,
                &mut session,
                label,
                destination,
                None,
                u64::try_from(index + 1).unwrap(),
            );
            if index == 0 {
                initial.capture_checkpoint(&mut session).unwrap();
            }
        }
        let initial_manifest = finish_candidate_index(&mut initial, &mut interner, &mut session);
        let old = initial_manifest.commit_for_test(session).unwrap();
        let old_committed = committed_reference_index(&arena, &old);

        source
            .apply_edit(SourceRevision(0), 0..0, "edited ")
            .unwrap();
        let candidate_source = source.descriptor();
        assert_eq!(candidate_source.revision, SourceRevision(1));
        assert_ne!(candidate_source.root, initial_source.root);

        let ticket = arena.begin_build().unwrap();
        let authority = AuthenticatedReferenceRestartAuthority::proof_only(
            &arena,
            ticket.id(),
            old_committed,
            1,
            4,
            candidate_source,
            12,
            2,
            88,
        )
        .unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let (mut restart, adoption) =
            ReferenceRestartIndexBuilder::new(&session, authority).unwrap();
        let mut interner = ReferenceLabelInterner::new_adopted(&mut session, adoption).unwrap();
        for (index, (label, destination, title)) in [
            (
                "beta",
                b"beta-new".as_slice(),
                Some(b"new title".as_slice()),
            ),
            ("epsilon", b"epsilon-from-gamma".as_slice(), None),
            ("alpha", b"alpha-later".as_slice(), None),
        ]
        .into_iter()
        .enumerate()
        {
            append_restart_cooked_occurrence(
                &mut restart,
                &mut interner,
                &mut session,
                label,
                destination,
                title,
                100 + u64::try_from(index).unwrap(),
            );
        }
        interner.begin_finish().unwrap();
        while interner.poll(&mut session).unwrap() != ReferenceLabelInternerProgress::ManifestReady
        {
        }
        restart
            .begin_finish(&session, interner.take_manifest().unwrap())
            .unwrap();

        // Deliberately withhold the scheduler for several actor turns, then
        // resume bounded reclaim between every subsequent suspended turn.
        let mut ticket = session.suspend().unwrap();
        let mut reclaim = ReferenceIndexReclaimService::try_new(32).unwrap();
        let mut delayed_pending = 0;
        for turn in 0..100_000_u64 {
            let mut session = arena.resume_build(ticket).unwrap();
            let progress = restart.poll(&mut session).unwrap();
            ticket = session.suspend().unwrap();
            if turn == 15 {
                delayed_pending = arena.metrics().pending_releases;
            }
            if turn >= 16 {
                reclaim.poll(&mut arena).unwrap();
            }
            if progress == ReferenceRestartIndexProgress::ManifestReady {
                break;
            }
        }
        assert!(delayed_pending > 0);
        let session = arena.resume_build(ticket).unwrap();
        let restarted = restart.take_manifest().unwrap();
        let receipt = restarted.receipt();
        assert_eq!(receipt.occurrences_spooled(), 3);
        assert_eq!(receipt.maximum_pending_occurrences(), 1);
        assert_eq!(receipt.document_sized_occurrence_vectors(), 0);
        assert_eq!(receipt.replacement_spool_roots(), 1);
        assert!(receipt.interner_adoption_proven());
        assert!(receipt.committed_exact_label_lookup_proven());
        assert!(receipt.changed_interval_streaming_proven());
        assert_eq!(receipt.suffix_occurrences_enumerated(), 0);
        assert_eq!(receipt.old_changed_occurrences_read, 3);
        assert_eq!(receipt.replacement_occurrences_reverse_read, 3);
        assert!(receipt.maximum_reverse_cursor_pages_per_task <= 1);
        assert!(receipt.maximum_branches_per_task <= 1);
        assert!(receipt.maximum_query_sequence_nodes_per_task <= 512);
        let next = restarted.commit_for_test(session).unwrap();
        let next_committed = committed_reference_index(&arena, &next);

        // The candidate owns every reused semantic edge. The old top-level
        // document may retire before any committed query runs.
        old.release_later(&mut arena).unwrap();
        while arena.metrics().pending_releases != 0 {
            reclaim.poll(&mut arena).unwrap();
        }
        assert!(reclaim.receipt().maximum_transitions_per_tick <= 32);
        assert!(reclaim.receipt().ticks > 0);

        let next_manifest = next.manifest(&arena).unwrap();
        assert_eq!(next_manifest.source_snapshot, Some(candidate_source));
        assert_eq!(next_manifest.interner_generation, 10);
        assert_eq!(next_manifest.occurrence_count, 7);
        assert_eq!(
            (0..next_manifest.occurrence_count)
                .map(|index| next.occurrence_at(&arena, index).unwrap().1.occurrence_id)
                .collect::<Vec<_>>(),
            vec![1, 8, 9, 10, 5, 6, 7]
        );

        let alpha = committed_winner(&arena, next_committed, "  ALPHA  ").unwrap();
        assert_eq!((alpha.occurrence_id(), alpha.label_id()), (1, 1));
        assert_eq!(
            read_committed_blob(&arena, alpha.destination(&arena).unwrap()),
            b"alpha-prefix"
        );
        let beta = committed_winner(&arena, next_committed, "beta").unwrap();
        assert_eq!((beta.occurrence_id(), beta.label_id()), (8, 2));
        assert_eq!(
            read_committed_blob(&arena, beta.destination(&arena).unwrap()),
            b"beta-new"
        );
        assert_eq!(
            read_committed_blob(&arena, beta.title(&arena).unwrap().unwrap()),
            b"new title"
        );
        let promoted = committed_winner(&arena, next_committed, "zeta").unwrap();
        assert_eq!((promoted.occurrence_id(), promoted.label_id()), (5, 3));
        assert_eq!(
            read_committed_blob(&arena, promoted.destination(&arena).unwrap()),
            b"zeta-promoted-suffix"
        );
        assert!(committed_winner(&arena, next_committed, "gamma").is_none());
        let relabelled = committed_winner(&arena, next_committed, "EPSILON").unwrap();
        assert_eq!((relabelled.occurrence_id(), relabelled.label_id()), (9, 6));
        assert_eq!(
            read_committed_blob(&arena, relabelled.destination(&arena).unwrap()),
            b"epsilon-from-gamma"
        );
        let suffix = committed_winner(&arena, next_committed, "delta").unwrap();
        assert_eq!(suffix.occurrence_id(), 7);
        assert_eq!(
            read_committed_blob(&arena, suffix.destination(&arena).unwrap()),
            b"delta-untouched-suffix"
        );

        next.release_later(&mut arena).unwrap();
        while arena.metrics().pending_releases != 0 {
            reclaim.poll(&mut arena).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn candidate_source_authority_rejects_zero_root_not_zero_revision() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let session = arena.resume_build(ticket).unwrap();
        let missing_root = SourceSnapshotDescriptor {
            revision: SourceRevision(0),
            root: SourceRootId(0),
            bytes: 0,
        };
        assert_eq!(
            ReferenceCandidateIndexBuilder::new(ReferenceCandidateIndexAuthority::proof_only(
                session.id(),
                missing_root,
                11,
                1,
                77,
            ))
            .unwrap_err(),
            RestartIndexError::Invalid("candidate reference-index authority is incomplete")
        );
        assert_eq!(
            encode_source_lineage_authority(11, missing_root).unwrap_err(),
            RestartIndexError::Invalid("source-lineage snapshot identity is incomplete")
        );
        assert_eq!(
            encode_document_manifest(
                0,
                0,
                0,
                0,
                0,
                0,
                9,
                11,
                1,
                0,
                Some(missing_root),
                false,
                false,
                false,
            )
            .unwrap_err(),
            RestartIndexError::Invalid("document source snapshot is incomplete or crossed")
        );
        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
    }

    #[test]
    fn persisted_document_rejects_same_revision_crossed_source_root_or_extent() {
        let document_source = SourceSnapshotDescriptor {
            revision: SourceRevision(0),
            root: SourceRootId(101),
            bytes: 1024,
        };
        assert_eq!(
            decode_crossed_source_manifest(
                document_source,
                SourceSnapshotDescriptor {
                    root: SourceRootId(102),
                    ..document_source
                },
            ),
            RestartIndexError::Invalid("document source snapshot crossed its lineage authority")
        );
        assert_eq!(
            decode_crossed_source_manifest(
                document_source,
                SourceSnapshotDescriptor {
                    bytes: 1025,
                    ..document_source
                },
            ),
            RestartIndexError::Invalid("document source snapshot crossed its lineage authority")
        );
    }

    #[test]
    fn source_lineage_extent_decode_checks_target_usize_width() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut payload = encode_authority(2, 11);
        payload[3] = 1;
        put_u64(&mut payload, 16, 101);
        put_u64(&mut payload, 24, u64::MAX);
        let (lineage, _) = session.allocate_packed(&payload, &[]).unwrap();
        let lineage_id = session.owner_id(&lineage).unwrap();
        let owner = session.commit(lineage).unwrap();
        if usize::BITS == 64 {
            assert_eq!(
                decode_source_lineage_authority(&arena, lineage_id).unwrap(),
                (11, Some((SourceRootId(101), usize::MAX)))
            );
        } else {
            assert_eq!(
                decode_source_lineage_authority(&arena, lineage_id).unwrap_err(),
                RestartIndexError::Invalid("persisted source byte extent exceeds usize")
            );
        }
        arena.release_later(owner).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn candidate_stream_zero_occurrences_finishes_without_collection_sentinels() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut interner = ReferenceLabelInterner::new_initial(session.id(), 9).unwrap();
        let mut builder = new_candidate_builder(&session);
        builder.capture_checkpoint(&mut session).unwrap();
        let manifest = finish_candidate_index(&mut builder, &mut interner, &mut session);
        let receipt = manifest.receipt();
        assert_eq!(receipt.occurrences_acknowledged(), 0);
        assert_eq!(receipt.exact_labels(), 0);
        assert_eq!(receipt.checkpoint_occurrences(), 0);
        assert_eq!(receipt.maximum_pending_occurrences(), 0);
        assert_eq!(receipt.document_sized_occurrence_vectors(), 0);
        assert_eq!(receipt.label_grouping_maps(), 0);
        assert!(!receipt.restart_interner_adoption_proven());
        assert!(!receipt.committed_exact_label_lookup_proven());
        assert!(!receipt.restart_changed_interval_streaming_proven());

        let document = manifest.commit_for_test(session).unwrap();
        let decoded = document.manifest(&arena).unwrap();
        assert_eq!(decoded.occurrence_count, 0);
        assert_eq!(decoded.restart_high_water, 0);
        assert_eq!(decoded.global_root, None);
        assert_eq!(decoded.directory_root, None);
        assert_eq!(
            decoded.source_snapshot,
            Some(SourceSnapshotDescriptor {
                revision: SourceRevision(1),
                root: SourceRootId(101),
                bytes: 1024,
            })
        );
        document.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn candidate_stream_one_occurrence_owns_cooked_destination_and_empty_title() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut interner = ReferenceLabelInterner::new_initial(session.id(), 9).unwrap();
        let mut builder = new_candidate_builder(&session);
        append_cooked_occurrence(
            &mut builder,
            &mut interner,
            &mut session,
            "label",
            b"https://example.invalid/a&amp;b",
            Some(b""),
            1,
        );
        builder.capture_checkpoint(&mut session).unwrap();
        let manifest = finish_candidate_index(&mut builder, &mut interner, &mut session);
        let receipt = manifest.receipt();
        assert_eq!(receipt.occurrences_acknowledged(), 1);
        assert_eq!(receipt.exact_labels(), 1);
        assert_eq!(receipt.maximum_pending_occurrences(), 1);
        assert!(receipt.maximum_branches_per_task() <= 1);
        let document = manifest.commit_for_test(session).unwrap();
        let (_, occurrence) = document.occurrence_at(&arena, 0).unwrap();
        let ReferenceOccurrenceValue::Cooked(value) = occurrence.value else {
            panic!("candidate builder published a fixture occurrence")
        };
        assert_eq!(value.destination_bytes, 31);
        assert!(value.destination_root.is_some());
        assert_eq!(
            value.title,
            Some(CookedReferenceBlobDescriptor {
                root: None,
                bytes: 0,
            })
        );
        assert_eq!(document.winner(&arena, 1).unwrap().unwrap().2, occurrence);
        document.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn candidate_stream_many_duplicates_preserves_winner_and_quiescent_checkpoint_prefix() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut interner = ReferenceLabelInterner::new_initial(session.id(), 9).unwrap();
        let mut builder = new_candidate_builder(&session);
        let definitions: [(&str, &[u8]); 5] = [
            ("b", b"b-first"),
            ("a", b"a-first"),
            ("b", b"b-second"),
            ("c", b"c-first"),
            ("a", b"a-second"),
        ];
        for (index, (label, destination)) in definitions.into_iter().enumerate() {
            append_cooked_occurrence(
                &mut builder,
                &mut interner,
                &mut session,
                label,
                destination,
                None,
                u64::try_from(index + 1).unwrap(),
            );
            if index == 1 {
                builder.capture_checkpoint(&mut session).unwrap();
            }
        }
        let manifest = finish_candidate_index(&mut builder, &mut interner, &mut session);
        let receipt = manifest.receipt();
        assert_eq!(receipt.occurrences_acknowledged(), 5);
        assert_eq!(receipt.exact_labels(), 3);
        assert_eq!(receipt.checkpoint_occurrences(), 2);
        assert_eq!(receipt.document_sized_occurrence_vectors(), 0);
        assert_eq!(receipt.label_grouping_maps(), 0);
        assert!(receipt.maximum_branches_per_task() <= 1);
        assert!(
            receipt.maximum_query_sequence_nodes_per_task() <= MAX_QUERY_SEQUENCE_NODES_PER_TASK
        );

        let document = manifest.commit_for_test(session).unwrap();
        let decoded = document.manifest(&arena).unwrap();
        assert_eq!(decoded.occurrence_count, 5);
        assert_eq!(decoded.restart_high_water, 2);
        assert_eq!(
            document
                .label_occurrences(&arena, 1)
                .unwrap()
                .into_iter()
                .map(|value| value.occurrence_id)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(
            document
                .label_occurrences(&arena, 2)
                .unwrap()
                .into_iter()
                .map(|value| value.occurrence_id)
                .collect::<Vec<_>>(),
            vec![2, 5]
        );
        assert_eq!(
            document.winner(&arena, 1).unwrap().unwrap().2.occurrence_id,
            1
        );
        assert_eq!(
            document.winner(&arena, 2).unwrap().unwrap().2.occurrence_id,
            2
        );

        let checkpoint = decode_checkpoint_manifest(&arena, decoded.checkpoint_root).unwrap();
        assert_eq!(checkpoint.occurrence_high_water, 2);
        assert_eq!(
            lookup_directory(&arena, checkpoint.prefix_directory, 1)
                .unwrap()
                .sequence_length,
            1
        );
        assert_eq!(
            lookup_directory(&arena, checkpoint.prefix_directory, 2)
                .unwrap()
                .sequence_length,
            1
        );
        assert!(
            lookup_directory(&arena, checkpoint.prefix_directory, 3)
                .unwrap()
                .leaf
                .is_none()
        );
        document.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn candidate_stream_every_bounded_task_fault_and_cancel_is_build_atomic() {
        let mut arena = PageArena::new();
        let CandidateAbortProbe::Complete(tasks) =
            run_candidate_abort_probe(&mut arena, None, None)
        else {
            panic!("fault-free candidate probe did not complete")
        };
        assert!(tasks > 20);
        assert_eq!(arena.metrics().live_nodes, 0);

        for task in 1..=tasks {
            assert_eq!(
                run_candidate_abort_probe(&mut arena, Some(task), None),
                CandidateAbortProbe::Fault(RestartIndexError::InjectedFault(task))
            );
            assert_eq!(arena.metrics().live_nodes, 0);
        }
        for task in 0..=tasks {
            assert_eq!(
                run_candidate_abort_probe(&mut arena, None, Some(task)),
                CandidateAbortProbe::Cancelled(task)
            );
            assert_eq!(arena.metrics().live_nodes, 0);
        }
    }

    #[test]
    fn candidate_stream_repeated_label_scale_stays_single_item_and_bounded() {
        const COUNT: usize = 65_536;
        const CHECKPOINT: usize = COUNT / 2;
        let started = std::time::Instant::now();

        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut interner = ReferenceLabelInterner::new_initial(session.id(), 9).unwrap();
        let mut builder = new_candidate_builder(&session);
        let mut reclaim_polls = 0_u64;
        let mut maximum_pending_releases = 0_usize;
        for index in 0..COUNT {
            append_cooked_occurrence(
                &mut builder,
                &mut interner,
                &mut session,
                "same",
                b"",
                None,
                u64::try_from(index + 1).unwrap(),
            );
            if index + 1 == CHECKPOINT {
                builder.capture_checkpoint(&mut session).unwrap();
            }
            if (index + 1).is_multiple_of(256) {
                let ticket = session.suspend().unwrap();
                maximum_pending_releases =
                    maximum_pending_releases.max(arena.metrics().pending_releases);
                while arena.metrics().pending_releases != 0 {
                    arena.poll_reclaim(512).unwrap();
                    reclaim_polls += 1;
                }
                session = arena.resume_build(ticket).unwrap();
            }
        }
        let manifest = finish_candidate_index(&mut builder, &mut interner, &mut session);
        let receipt = manifest.receipt();
        assert_eq!(receipt.occurrences_acknowledged(), COUNT as u64);
        assert_eq!(receipt.exact_labels(), 1);
        assert_eq!(receipt.checkpoint_occurrences(), CHECKPOINT as u64);
        assert_eq!(receipt.maximum_pending_occurrences(), 1);
        assert_eq!(receipt.document_sized_occurrence_vectors(), 0);
        assert_eq!(receipt.label_grouping_maps(), 0);
        assert!(!receipt.restart_interner_adoption_proven());
        assert!(!receipt.committed_exact_label_lookup_proven());
        assert!(!receipt.restart_changed_interval_streaming_proven());
        assert!(receipt.maximum_branches_per_task() <= 1, "{receipt:?}");
        assert!(
            receipt.maximum_query_sequence_nodes_per_task() <= MAX_QUERY_SEQUENCE_NODES_PER_TASK,
            "{receipt:?}"
        );
        eprintln!(
            "65,536-occurrence initial reference stream: elapsed={:?}, reclaim_polls={reclaim_polls}, maximum_pending_releases={maximum_pending_releases}, receipt={receipt:?}",
            started.elapsed(),
        );

        let document = manifest.commit_for_test(session).unwrap();
        assert_eq!(
            document
                .occurrence_at(&arena, u64::try_from(COUNT - 1).unwrap())
                .unwrap()
                .1
                .occurrence_id,
            COUNT as u64
        );
        assert_eq!(
            document.winner(&arena, 1).unwrap().unwrap().2.occurrence_id,
            1
        );
        document.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }
}
