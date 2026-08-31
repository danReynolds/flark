//! Persistent, source-measured storage for parser-owned block coverage.
//!
//! The parser contributes authority-free, leaf-relative Green and Projection
//! records. This module binds their ordered coverage to one exact immutable
//! source, packs a bounded number of semantic entries into each arena page,
//! and owns the persistent measured tree used for logarithmic point routing.

use std::fmt;
use std::ops::Range;

use crate::document::DocumentRuntime;
use crate::identity::ArenaId;
use crate::identity::RuntimeIdentity;
use crate::measured_sequence::{
    begin_measured_sequence_seal, maximum_metric_lookup_node_headers,
    retain_committed_measured_sequence_root_with_measure, splice_measured_sequence_atomic,
    splice_measured_sequence_build_root_atomic, validate_measured_sequence_build_owner,
    BeginMeasuredSequenceSealFailure, CommittedMeasuredSequenceRoot, MeasuredSequenceBuildRoot,
    MeasuredSequenceRef, MeasuredSequenceSeal, ResumableMeasuredSequenceBuilder,
    ResumableSequenceProgress, SequenceInspectionReceipt, SequenceLeafVisitControl,
    SequenceMeasure, SequenceMutationReceipt, SequenceSpec, SequenceSpecInspection,
};
use crate::mersenne61::{add_mod, multiply_mod, MODULUS as COMMITMENT_MODULUS};
use crate::source::{SourceBoundaryAffinity, SourceEditError, SourceSnapshotLease, SourceVersion};
use crate::storage::{ArenaBuildOwner, ArenaBuildSession, ArenaError, CandidateBuild, PageArena};
use crate::{ReclaimReceipt, ARENA_PAGE_BYTES};

const BLOCK_LEAF_MAGIC: [u8; 4] = *b"BSL1";
const BLOCK_BRANCH_MAGIC: [u8; 4] = *b"BSB1";
const BLOCK_ROLE_DESCRIPTOR_MAGIC: [u8; 4] = *b"BRD1";
const BLOCK_SEQUENCE_SCHEMA: u32 = 2;
const BLOCK_ENTRY_HEADER_BYTES: usize = 34;
const BLOCK_LANE_CANONICAL_HEADER_BYTES: u64 = 32;
const BLOCK_SUMMARY_COUNTERS: usize = 11;
const COMMITMENT_LANES: usize = 4;
const COMMITMENT_BYTES: usize = COMMITMENT_LANES * 2 * 8;
const BLOCK_LEAF_HEADER_BYTES: usize =
    4 + 4 + 2 + 2 + 4 + BLOCK_SUMMARY_COUNTERS * 8 + 2 * COMMITMENT_BYTES;
const BLOCK_BRANCH_BYTES: usize = 4 + 4 + 8 + 2 + BLOCK_SUMMARY_COUNTERS * 8 + 2 * COMMITMENT_BYTES;
const COMMITMENT_BASES: [u64; COMMITMENT_LANES] = [
    0x0a09_e667_f3bc_c909,
    0x1b67_ae85_84ca_a73b,
    0x1c6e_f372_fe94_f82b,
    0x154f_f53a_5f1d_36f1,
];

/// Maximum opaque bytes in either one Green or one Projection block record.
pub const M11_BLOCK_SEQUENCE_MAX_ROLE_RECORD_BYTES: usize = 256;
/// Maximum semantic entries decoded after the logarithmic measured-tree walk.
pub const M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX: usize = 64;
/// Maximum build or reclamation transitions accepted by one public poll.
pub const M11_BLOCK_SEQUENCE_MAX_POLL_TRANSITIONS: usize = 4096;
pub(crate) const PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA: u32 = 2;
pub(crate) const PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA: u32 = 3;
pub(crate) const PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES: usize = 176;

/// Conservative header-decode admission bound for one contiguous page visit.
///
/// The visited structural union contains at most one boundary path plus the
/// binary subtree connecting the admitted leaves. The coefficients also cover
/// the root preflight and the direct-child headers authenticated by each
/// checked branch decode.
pub(crate) const fn maximum_consecutive_block_visit_node_headers(
    tree_height: u16,
    maximum_storage_pages: u32,
) -> u64 {
    7 * maximum_storage_pages as u64 + 3 * tree_height as u64 + 3
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum M11BlockRoleLane {
    Green = 1,
    Projection = 2,
}

impl M11BlockRoleLane {
    fn decode(value: u8) -> Result<Self, M11BlockSequenceError> {
        match value {
            1 => Ok(Self::Green),
            2 => Ok(Self::Projection),
            _ => Err(M11BlockSequenceError::Corrupt(
                "block role descriptor lane is unsupported",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LaneCommitment {
    hash: [u64; COMMITMENT_LANES],
    factor: [u64; COMMITMENT_LANES],
}

impl LaneCommitment {
    const fn empty() -> Self {
        Self {
            hash: [0; COMMITMENT_LANES],
            factor: [1; COMMITMENT_LANES],
        }
    }

    fn for_digest(digest: [u8; 32]) -> Self {
        let mut hash = [0_u64; COMMITMENT_LANES];
        for (lane, value) in hash.iter_mut().enumerate() {
            let start = lane * 8;
            let coefficient = u64::from_le_bytes(
                digest[start..start + 8]
                    .try_into()
                    .expect("BLAKE3 lane has eight bytes"),
            );
            *value = coefficient % (COMMITMENT_MODULUS - 1) + 1;
        }
        Self {
            hash,
            factor: COMMITMENT_BASES,
        }
    }

    const fn combine(self, right: Self) -> Self {
        let mut hash = [0; COMMITMENT_LANES];
        let mut factor = [0; COMMITMENT_LANES];
        let mut lane = 0;
        while lane < COMMITMENT_LANES {
            hash[lane] = add_mod(
                multiply_mod(self.hash[lane], right.factor[lane]),
                right.hash[lane],
            );
            factor[lane] = multiply_mod(self.factor[lane], right.factor[lane]);
            lane += 1;
        }
        Self { hash, factor }
    }

    fn checksum(self) -> [u8; 32] {
        let mut checksum = [0_u8; 32];
        for (lane, value) in self.hash.into_iter().enumerate() {
            let start = lane * 8;
            checksum[start..start + 8].copy_from_slice(&value.to_le_bytes());
        }
        checksum
    }
}

/// Shape-independent semantic summary stored in every measured branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BlockSequenceSummary {
    source_bytes: u64,
    source_utf16: u64,
    entries: u64,
    paragraphs: u64,
    structured: u64,
    blanks: u64,
    definitions_only: u64,
    unsupported: u64,
    reference_definitions: u64,
    green_canonical_bytes: u64,
    projection_canonical_bytes: u64,
    green: LaneCommitment,
    projection: LaneCommitment,
}

impl BlockSequenceSummary {
    const fn empty() -> Self {
        Self {
            source_bytes: 0,
            source_utf16: 0,
            entries: 0,
            paragraphs: 0,
            structured: 0,
            blanks: 0,
            definitions_only: 0,
            unsupported: 0,
            reference_definitions: 0,
            green_canonical_bytes: 0,
            projection_canonical_bytes: 0,
            green: LaneCommitment::empty(),
            projection: LaneCommitment::empty(),
        }
    }

    fn checked_followed_by(self, right: Self) -> Result<Self, M11BlockSequenceError> {
        Ok(Self {
            source_bytes: self
                .source_bytes
                .checked_add(right.source_bytes)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            source_utf16: self
                .source_utf16
                .checked_add(right.source_utf16)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            entries: self
                .entries
                .checked_add(right.entries)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            paragraphs: self
                .paragraphs
                .checked_add(right.paragraphs)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            structured: self
                .structured
                .checked_add(right.structured)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            blanks: self
                .blanks
                .checked_add(right.blanks)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            definitions_only: self
                .definitions_only
                .checked_add(right.definitions_only)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            unsupported: self
                .unsupported
                .checked_add(right.unsupported)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            reference_definitions: self
                .reference_definitions
                .checked_add(right.reference_definitions)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            green_canonical_bytes: self
                .green_canonical_bytes
                .checked_add(right.green_canonical_bytes)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            projection_canonical_bytes: self
                .projection_canonical_bytes
                .checked_add(right.projection_canonical_bytes)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            green: self.green.combine(right.green),
            projection: self.projection.combine(right.projection),
        })
    }
}

struct BlockSequenceSpec;

impl SequenceSpec for BlockSequenceSpec {
    type Summary = BlockSequenceSummary;
    type Error = M11BlockSequenceError;

    fn leaf_summary(
        payload: &[u8],
        inspection: &mut SequenceSpecInspection,
    ) -> Result<Option<Self::Summary>, Self::Error> {
        decode_leaf(payload, inspection).map(|leaf| leaf.map(|leaf| leaf.summary))
    }

    fn branch_measure(
        payload: &[u8],
        inspection: &mut SequenceSpecInspection,
    ) -> Result<Option<SequenceMeasure<Self::Summary>>, Self::Error> {
        if payload.get(..4) != Some(BLOCK_BRANCH_MAGIC.as_slice()) {
            return Ok(None);
        }
        if payload.len() != BLOCK_BRANCH_BYTES {
            return Err(M11BlockSequenceError::Corrupt(
                "block branch has the wrong length",
            ));
        }
        inspection
            .charge_payload_bytes(payload.len())
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        let mut cursor = 4;
        let schema = read_u32(payload, &mut cursor)?;
        let leaves = read_u64(payload, &mut cursor)?;
        let height = read_u16(payload, &mut cursor)?;
        let summary = decode_summary(payload, &mut cursor)?;
        if schema != BLOCK_SEQUENCE_SCHEMA
            || leaves < 2
            || height < 2
            || summary.entries == 0
            || summary.source_bytes == 0
            || cursor != payload.len()
        {
            return Err(M11BlockSequenceError::Corrupt(
                "block branch metadata is invalid",
            ));
        }
        validate_summary_counts(summary)?;
        Ok(Some(SequenceMeasure::new(summary, leaves, height)))
    }

    fn encode_branch(
        measure: SequenceMeasure<Self::Summary>,
        output: &mut [u8; ARENA_PAGE_BYTES],
    ) -> Result<usize, Self::Error> {
        let summary = measure.summary();
        validate_summary_counts(summary)?;
        if measure.leaves() < 2
            || measure.height() < 2
            || summary.entries == 0
            || summary.source_bytes == 0
        {
            return Err(M11BlockSequenceError::Corrupt(
                "block branch measure is invalid",
            ));
        }
        let mut cursor = 0;
        write_bytes(output, &mut cursor, &BLOCK_BRANCH_MAGIC)?;
        write_u32(output, &mut cursor, BLOCK_SEQUENCE_SCHEMA)?;
        write_u64(output, &mut cursor, measure.leaves())?;
        write_u16(output, &mut cursor, measure.height())?;
        encode_summary(summary, output, &mut cursor)?;
        if cursor != BLOCK_BRANCH_BYTES {
            return Err(M11BlockSequenceError::Corrupt(
                "block branch encoding length changed",
            ));
        }
        Ok(cursor)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        validate_summary_counts(left)?;
        validate_summary_counts(right)?;
        left.checked_followed_by(right)
    }

    fn invalid(message: &'static str) -> Self::Error {
        M11BlockSequenceError::Corrupt(message)
    }
}

type BlockSequenceBuilder = ResumableMeasuredSequenceBuilder<BlockSequenceSpec>;
type BlockSequenceBuildRoot = MeasuredSequenceBuildRoot<BlockSequenceSpec>;
type BlockSequenceSeal = MeasuredSequenceSeal<BlockSequenceSpec>;
type BlockSequenceTree = CommittedMeasuredSequenceRoot<BlockSequenceSpec>;

/// The semantic kind of one exact source-coverage entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum M11BlockSequenceEntryKind {
    Paragraph = 1,
    Blank = 2,
    DefinitionsOnly = 3,
    Unsupported = 4,
    Structured = 5,
}

impl M11BlockSequenceEntryKind {
    fn decode(value: u8) -> Result<Self, M11BlockSequenceError> {
        match value {
            1 => Ok(Self::Paragraph),
            2 => Ok(Self::Blank),
            3 => Ok(Self::DefinitionsOnly),
            4 => Ok(Self::Unsupported),
            5 => Ok(Self::Structured),
            _ => Err(M11BlockSequenceError::Corrupt(
                "block entry kind is unsupported",
            )),
        }
    }
}

/// Stable parser-defined reason code carried by an unsupported coverage entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11BlockUnsupportedReason(u32);

impl M11BlockUnsupportedReason {
    pub fn new(code: u32) -> Result<Self, M11BlockSequenceError> {
        if code == 0 {
            return Err(M11BlockSequenceError::InvalidUnsupportedReason);
        }
        Ok(Self(code))
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Fixed-capacity authority-free bytes for one Green or Projection record.
#[derive(Clone, Eq, PartialEq)]
pub struct M11BlockRoleRecord {
    bytes: [u8; M11_BLOCK_SEQUENCE_MAX_ROLE_RECORD_BYTES],
    len: u16,
}

impl fmt::Debug for M11BlockRoleRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BlockRoleRecord")
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl M11BlockRoleRecord {
    pub fn new(bytes: &[u8]) -> Result<Self, M11BlockSequenceError> {
        if bytes.is_empty() {
            return Err(M11BlockSequenceError::RoleRecordEmpty);
        }
        if bytes.len() > M11_BLOCK_SEQUENCE_MAX_ROLE_RECORD_BYTES {
            return Err(M11BlockSequenceError::RoleRecordTooLarge {
                bytes: bytes.len(),
                cap: M11_BLOCK_SEQUENCE_MAX_ROLE_RECORD_BYTES,
            });
        }
        let mut storage = [0_u8; M11_BLOCK_SEQUENCE_MAX_ROLE_RECORD_BYTES];
        storage[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            bytes: storage,
            len: u16::try_from(bytes.len()).expect("record cap fits u16"),
        })
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

/// One contiguous, scalar-aligned slice of complete document coverage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11BlockSequenceEntry {
    kind: M11BlockSequenceEntryKind,
    source_bytes: u64,
    source_utf16: u64,
    reference_definition_count: u64,
    green: Option<M11BlockRoleRecord>,
    projection: Option<M11BlockRoleRecord>,
    unsupported_reason: Option<M11BlockUnsupportedReason>,
}

impl M11BlockSequenceEntry {
    pub fn paragraph(
        source_bytes: usize,
        source_utf16: usize,
        reference_definition_count: u64,
        green: M11BlockRoleRecord,
        projection: M11BlockRoleRecord,
    ) -> Result<Self, M11BlockSequenceError> {
        Self::new(
            M11BlockSequenceEntryKind::Paragraph,
            source_bytes,
            source_utf16,
            reference_definition_count,
            Some(green),
            Some(projection),
            None,
        )
    }

    pub fn structured(
        source_bytes: usize,
        source_utf16: usize,
        reference_definition_count: u64,
        green: M11BlockRoleRecord,
        projection: M11BlockRoleRecord,
    ) -> Result<Self, M11BlockSequenceError> {
        Self::new(
            M11BlockSequenceEntryKind::Structured,
            source_bytes,
            source_utf16,
            reference_definition_count,
            Some(green),
            Some(projection),
            None,
        )
    }

    pub fn blank(source_bytes: usize, source_utf16: usize) -> Result<Self, M11BlockSequenceError> {
        Self::new(
            M11BlockSequenceEntryKind::Blank,
            source_bytes,
            source_utf16,
            0,
            None,
            None,
            None,
        )
    }

    pub fn definitions_only(
        source_bytes: usize,
        source_utf16: usize,
        reference_definition_count: u64,
    ) -> Result<Self, M11BlockSequenceError> {
        Self::new(
            M11BlockSequenceEntryKind::DefinitionsOnly,
            source_bytes,
            source_utf16,
            reference_definition_count,
            None,
            None,
            None,
        )
    }

    pub fn unsupported(
        source_bytes: usize,
        source_utf16: usize,
        reason: M11BlockUnsupportedReason,
    ) -> Result<Self, M11BlockSequenceError> {
        Self::new(
            M11BlockSequenceEntryKind::Unsupported,
            source_bytes,
            source_utf16,
            0,
            None,
            None,
            Some(reason),
        )
    }

    fn new(
        kind: M11BlockSequenceEntryKind,
        source_bytes: usize,
        source_utf16: usize,
        reference_definition_count: u64,
        green: Option<M11BlockRoleRecord>,
        projection: Option<M11BlockRoleRecord>,
        unsupported_reason: Option<M11BlockUnsupportedReason>,
    ) -> Result<Self, M11BlockSequenceError> {
        if source_bytes == 0 || source_utf16 == 0 || source_utf16 > source_bytes {
            return Err(M11BlockSequenceError::InvalidEntryCoverage);
        }
        if matches!(
            kind,
            M11BlockSequenceEntryKind::Blank | M11BlockSequenceEntryKind::Unsupported
        ) && reference_definition_count != 0
        {
            return Err(M11BlockSequenceError::InvalidEntryShape);
        }
        if matches!(kind, M11BlockSequenceEntryKind::DefinitionsOnly)
            && reference_definition_count == 0
        {
            return Err(M11BlockSequenceError::InvalidEntryShape);
        }
        let records_match_kind = if matches!(
            kind,
            M11BlockSequenceEntryKind::Paragraph | M11BlockSequenceEntryKind::Structured
        ) {
            green.is_some() && projection.is_some()
        } else {
            green.is_none() && projection.is_none()
        };
        if !records_match_kind
            || matches!(kind, M11BlockSequenceEntryKind::Unsupported)
                != unsupported_reason.is_some()
        {
            return Err(M11BlockSequenceError::InvalidEntryShape);
        }
        Ok(Self {
            kind,
            source_bytes: u64::try_from(source_bytes)
                .map_err(|_| M11BlockSequenceError::CounterOverflow)?,
            source_utf16: u64::try_from(source_utf16)
                .map_err(|_| M11BlockSequenceError::CounterOverflow)?,
            reference_definition_count,
            green,
            projection,
            unsupported_reason,
        })
    }

    #[must_use]
    pub const fn kind(&self) -> M11BlockSequenceEntryKind {
        self.kind
    }

    #[must_use]
    pub const fn source_byte_len(&self) -> u64 {
        self.source_bytes
    }

    #[must_use]
    pub const fn source_utf16_len(&self) -> u64 {
        self.source_utf16
    }

    #[must_use]
    pub const fn reference_definition_count(&self) -> u64 {
        self.reference_definition_count
    }

    #[must_use]
    pub fn green(&self) -> Option<&M11BlockRoleRecord> {
        self.green.as_ref()
    }

    #[must_use]
    pub fn projection(&self) -> Option<&M11BlockRoleRecord> {
        self.projection.as_ref()
    }

    #[must_use]
    pub const fn unsupported_reason(&self) -> Option<M11BlockUnsupportedReason> {
        self.unsupported_reason
    }

    fn encoded_len(&self) -> usize {
        BLOCK_ENTRY_HEADER_BYTES
            + self
                .green
                .as_ref()
                .map_or(0, |record| record.as_bytes().len())
            + self
                .projection
                .as_ref()
                .map_or(0, |record| record.as_bytes().len())
    }
}

/// Authority, lifecycle, resource, or canonical block-sequence failure.
#[derive(Debug)]
pub enum M11BlockSequenceError {
    InvalidEntryCoverage,
    InvalidEntryShape,
    InvalidUnsupportedReason,
    RoleRecordEmpty,
    RoleRecordTooLarge { bytes: usize, cap: usize },
    EntryAlreadyPending,
    InputClosed,
    IncompleteCoverage,
    InvalidPoint,
    InvalidState,
    WrongRuntime,
    SourceAuthorityMismatch,
    ZeroFuel,
    PollLimitExceeded,
    CounterOverflow,
    Corrupt(&'static str),
    Arena(ArenaError),
    Source(SourceEditError),
}

impl fmt::Display for M11BlockSequenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEntryCoverage => {
                formatter.write_str("block entry coverage must be a nonempty scalar-aligned range")
            }
            Self::InvalidEntryShape => {
                formatter.write_str("block entry records do not match its semantic kind")
            }
            Self::InvalidUnsupportedReason => {
                formatter.write_str("unsupported block reason must be nonzero")
            }
            Self::RoleRecordEmpty => formatter.write_str("block role records must not be empty"),
            Self::RoleRecordTooLarge { bytes, cap } => {
                write!(
                    formatter,
                    "block role record has {bytes} bytes above the {cap}-byte cap"
                )
            }
            Self::EntryAlreadyPending => {
                formatter.write_str("a block sequence entry is already pending")
            }
            Self::InputClosed => formatter.write_str("block sequence input is already closed"),
            Self::IncompleteCoverage => {
                formatter.write_str("block sequence does not exactly cover its source")
            }
            Self::InvalidPoint => formatter.write_str("block sequence point is invalid"),
            Self::InvalidState => formatter.write_str("block sequence owner is in the wrong state"),
            Self::WrongRuntime => {
                formatter.write_str("block sequence owner belongs to another document runtime")
            }
            Self::SourceAuthorityMismatch => {
                formatter.write_str("block sequence source authority is not current")
            }
            Self::ZeroFuel => formatter.write_str("block sequence poll requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("block sequence poll exceeds the bounded transition limit")
            }
            Self::CounterOverflow => formatter.write_str("block sequence counter overflow"),
            Self::Corrupt(message) => write!(formatter, "corrupt block sequence: {message}"),
            Self::Arena(error) => error.fmt(formatter),
            Self::Source(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11BlockSequenceError {}

impl From<ArenaError> for M11BlockSequenceError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<SourceEditError> for M11BlockSequenceError {
    fn from(value: SourceEditError) -> Self {
        Self::Source(value)
    }
}

fn validate_summary_counts(summary: BlockSequenceSummary) -> Result<(), M11BlockSequenceError> {
    let count = summary
        .paragraphs
        .checked_add(summary.structured)
        .and_then(|count| count.checked_add(summary.blanks))
        .and_then(|count| count.checked_add(summary.definitions_only))
        .and_then(|count| count.checked_add(summary.unsupported))
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    let minimum_lane_bytes = summary
        .entries
        .checked_mul(BLOCK_LANE_CANONICAL_HEADER_BYTES)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    let commitments_valid = [summary.green, summary.projection]
        .into_iter()
        .all(|commitment| {
            commitment
                .hash
                .into_iter()
                .all(|value| value < COMMITMENT_MODULUS)
                && commitment
                    .factor
                    .into_iter()
                    .all(|value| value > 0 && value < COMMITMENT_MODULUS)
        });
    if count != summary.entries
        || (summary.entries == 0) != (summary.source_bytes == 0)
        || summary.source_utf16 > summary.source_bytes
        || summary.green_canonical_bytes < minimum_lane_bytes
        || summary.projection_canonical_bytes < minimum_lane_bytes
        || !commitments_valid
        || (summary.entries == 0
            && (summary != BlockSequenceSummary::empty() || summary.reference_definitions != 0))
    {
        return Err(M11BlockSequenceError::Corrupt(
            "block summary counters are inconsistent",
        ));
    }
    Ok(())
}

fn entry_summary(entry: &M11BlockSequenceEntry) -> BlockSequenceSummary {
    let (paragraphs, structured, blanks, definitions_only, unsupported) = match entry.kind {
        M11BlockSequenceEntryKind::Paragraph => (1, 0, 0, 0, 0),
        M11BlockSequenceEntryKind::Structured => (0, 1, 0, 0, 0),
        M11BlockSequenceEntryKind::Blank => (0, 0, 1, 0, 0),
        M11BlockSequenceEntryKind::DefinitionsOnly => (0, 0, 0, 1, 0),
        M11BlockSequenceEntryKind::Unsupported => (0, 0, 0, 0, 1),
    };
    let green_record_bytes = entry.green.as_ref().map_or(0, |record| {
        u64::try_from(record.as_bytes().len()).expect("record cap fits u64")
    });
    let projection_record_bytes = entry.projection.as_ref().map_or(0, |record| {
        u64::try_from(record.as_bytes().len()).expect("record cap fits u64")
    });
    BlockSequenceSummary {
        source_bytes: entry.source_bytes,
        source_utf16: entry.source_utf16,
        entries: 1,
        paragraphs,
        structured,
        blanks,
        definitions_only,
        unsupported,
        reference_definitions: entry.reference_definition_count,
        green_canonical_bytes: BLOCK_LANE_CANONICAL_HEADER_BYTES + green_record_bytes,
        projection_canonical_bytes: BLOCK_LANE_CANONICAL_HEADER_BYTES + projection_record_bytes,
        green: entry_lane_commitment(entry, true),
        projection: entry_lane_commitment(entry, false),
    }
}

fn entry_lane_commitment(entry: &M11BlockSequenceEntry, green: bool) -> LaneCommitment {
    let record = if green {
        entry.green.as_ref()
    } else {
        entry.projection.as_ref()
    };
    let lane = if green { b'G' } else { b'P' };
    let reason = entry
        .unsupported_reason
        .map_or(0, M11BlockUnsupportedReason::get);
    let record_len = record.map_or(0_u16, |record| {
        u16::try_from(record.as_bytes().len()).expect("record cap fits u16")
    });
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.block-sequence.lane-entry.v1\0");
    hasher.update(&[lane, entry.kind as u8]);
    hasher.update(&entry.source_bytes.to_le_bytes());
    hasher.update(&entry.source_utf16.to_le_bytes());
    hasher.update(&entry.reference_definition_count.to_le_bytes());
    hasher.update(&reason.to_le_bytes());
    hasher.update(&record_len.to_le_bytes());
    if let Some(record) = record {
        hasher.update(record.as_bytes());
    }
    LaneCommitment::for_digest(*hasher.finalize().as_bytes())
}

fn encode_entry(
    entry: &M11BlockSequenceEntry,
    output: &mut [u8],
    cursor: &mut usize,
) -> Result<(), M11BlockSequenceError> {
    let green_len = entry
        .green
        .as_ref()
        .map_or(0, |record| record.as_bytes().len());
    let projection_len = entry
        .projection
        .as_ref()
        .map_or(0, |record| record.as_bytes().len());
    write_u8(output, cursor, entry.kind as u8)?;
    write_u8(output, cursor, 0)?;
    write_u16(
        output,
        cursor,
        u16::try_from(green_len).expect("record cap fits u16"),
    )?;
    write_u16(
        output,
        cursor,
        u16::try_from(projection_len).expect("record cap fits u16"),
    )?;
    write_u32(
        output,
        cursor,
        entry
            .unsupported_reason
            .map_or(0, M11BlockUnsupportedReason::get),
    )?;
    write_u64(output, cursor, entry.source_bytes)?;
    write_u64(output, cursor, entry.source_utf16)?;
    write_u64(output, cursor, entry.reference_definition_count)?;
    if let Some(record) = entry.green.as_ref() {
        write_bytes(output, cursor, record.as_bytes())?;
    }
    if let Some(record) = entry.projection.as_ref() {
        write_bytes(output, cursor, record.as_bytes())?;
    }
    Ok(())
}

fn decode_entry(
    input: &[u8],
    cursor: &mut usize,
) -> Result<M11BlockSequenceEntry, M11BlockSequenceError> {
    let kind = M11BlockSequenceEntryKind::decode(read_u8(input, cursor)?)?;
    if read_u8(input, cursor)? != 0 {
        return Err(M11BlockSequenceError::Corrupt(
            "block entry reserved byte is nonzero",
        ));
    }
    let green_len = usize::from(read_u16(input, cursor)?);
    let projection_len = usize::from(read_u16(input, cursor)?);
    let reason = read_u32(input, cursor)?;
    let source_bytes = read_u64(input, cursor)?;
    let source_utf16 = read_u64(input, cursor)?;
    let reference_definition_count = read_u64(input, cursor)?;
    if green_len > M11_BLOCK_SEQUENCE_MAX_ROLE_RECORD_BYTES
        || projection_len > M11_BLOCK_SEQUENCE_MAX_ROLE_RECORD_BYTES
    {
        return Err(M11BlockSequenceError::Corrupt(
            "block entry role record exceeds its cap",
        ));
    }
    let green = if green_len == 0 {
        None
    } else {
        Some(M11BlockRoleRecord::new(read_bytes(
            input, cursor, green_len,
        )?)?)
    };
    let projection = if projection_len == 0 {
        None
    } else {
        Some(M11BlockRoleRecord::new(read_bytes(
            input,
            cursor,
            projection_len,
        )?)?)
    };
    let unsupported_reason = if reason == 0 {
        None
    } else {
        Some(M11BlockUnsupportedReason::new(reason)?)
    };
    M11BlockSequenceEntry::new(
        kind,
        usize::try_from(source_bytes).map_err(|_| M11BlockSequenceError::CounterOverflow)?,
        usize::try_from(source_utf16).map_err(|_| M11BlockSequenceError::CounterOverflow)?,
        reference_definition_count,
        green,
        projection,
        unsupported_reason,
    )
    .map_err(|_| M11BlockSequenceError::Corrupt("block entry shape is invalid"))
}

#[derive(Clone, Copy)]
struct DecodedLeaf {
    summary: BlockSequenceSummary,
    entries: u16,
    entries_start: usize,
    entries_end: usize,
}

fn decode_leaf(
    payload: &[u8],
    inspection: &mut SequenceSpecInspection,
) -> Result<Option<DecodedLeaf>, M11BlockSequenceError> {
    if payload.get(..4) != Some(BLOCK_LEAF_MAGIC.as_slice()) {
        return Ok(None);
    }
    if payload.len() < BLOCK_LEAF_HEADER_BYTES || payload.len() > ARENA_PAGE_BYTES {
        return Err(M11BlockSequenceError::Corrupt(
            "block leaf has an invalid length",
        ));
    }
    inspection
        .charge_payload_bytes(payload.len())
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    let mut cursor = 4;
    if read_u32(payload, &mut cursor)? != BLOCK_SEQUENCE_SCHEMA {
        return Err(M11BlockSequenceError::Corrupt(
            "block leaf schema is unsupported",
        ));
    }
    let entries = read_u16(payload, &mut cursor)?;
    if read_u16(payload, &mut cursor)? != 0 {
        return Err(M11BlockSequenceError::Corrupt(
            "block leaf reserved bytes are nonzero",
        ));
    }
    let encoded_entry_bytes = usize::try_from(read_u32(payload, &mut cursor)?)
        .map_err(|_| M11BlockSequenceError::CounterOverflow)?;
    let claimed = decode_summary(payload, &mut cursor)?;
    if cursor != BLOCK_LEAF_HEADER_BYTES
        || entries == 0
        || usize::from(entries) > M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX
        || BLOCK_LEAF_HEADER_BYTES
            .checked_add(encoded_entry_bytes)
            .is_none_or(|end| end != payload.len())
    {
        return Err(M11BlockSequenceError::Corrupt(
            "block leaf header is invalid",
        ));
    }
    let entries_start = cursor;
    let mut actual = BlockSequenceSummary::empty();
    for _ in 0..entries {
        let entry = decode_entry(payload, &mut cursor)?;
        actual = actual.checked_followed_by(entry_summary(&entry))?;
    }
    inspection
        .charge_hashed_items(usize::from(entries))
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    if cursor != payload.len() || actual != claimed {
        return Err(M11BlockSequenceError::Corrupt(
            "block leaf summary differs from its entries",
        ));
    }
    Ok(Some(DecodedLeaf {
        summary: actual,
        entries,
        entries_start,
        entries_end: cursor,
    }))
}

fn encode_leaf_header(
    page: &mut [u8; ARENA_PAGE_BYTES],
    entries: u16,
    encoded_entry_bytes: usize,
    summary: BlockSequenceSummary,
) -> Result<(), M11BlockSequenceError> {
    validate_summary_counts(summary)?;
    if entries == 0
        || usize::from(entries) > M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX
        || summary.entries != u64::from(entries)
        || summary.source_bytes == 0
    {
        return Err(M11BlockSequenceError::Corrupt(
            "block leaf summary is invalid",
        ));
    }
    let mut cursor = 0;
    write_bytes(page, &mut cursor, &BLOCK_LEAF_MAGIC)?;
    write_u32(page, &mut cursor, BLOCK_SEQUENCE_SCHEMA)?;
    write_u16(page, &mut cursor, entries)?;
    write_u16(page, &mut cursor, 0)?;
    write_u32(
        page,
        &mut cursor,
        u32::try_from(encoded_entry_bytes).map_err(|_| M11BlockSequenceError::CounterOverflow)?,
    )?;
    encode_summary(summary, page, &mut cursor)?;
    if cursor != BLOCK_LEAF_HEADER_BYTES {
        return Err(M11BlockSequenceError::Corrupt(
            "block leaf header encoding length changed",
        ));
    }
    Ok(())
}

fn encode_summary(
    summary: BlockSequenceSummary,
    output: &mut [u8],
    cursor: &mut usize,
) -> Result<(), M11BlockSequenceError> {
    write_u64(output, cursor, summary.source_bytes)?;
    write_u64(output, cursor, summary.source_utf16)?;
    write_u64(output, cursor, summary.entries)?;
    write_u64(output, cursor, summary.paragraphs)?;
    write_u64(output, cursor, summary.structured)?;
    write_u64(output, cursor, summary.blanks)?;
    write_u64(output, cursor, summary.definitions_only)?;
    write_u64(output, cursor, summary.unsupported)?;
    write_u64(output, cursor, summary.reference_definitions)?;
    write_u64(output, cursor, summary.green_canonical_bytes)?;
    write_u64(output, cursor, summary.projection_canonical_bytes)?;
    encode_commitment(summary.green, output, cursor)?;
    encode_commitment(summary.projection, output, cursor)
}

fn decode_summary(
    input: &[u8],
    cursor: &mut usize,
) -> Result<BlockSequenceSummary, M11BlockSequenceError> {
    Ok(BlockSequenceSummary {
        source_bytes: read_u64(input, cursor)?,
        source_utf16: read_u64(input, cursor)?,
        entries: read_u64(input, cursor)?,
        paragraphs: read_u64(input, cursor)?,
        structured: read_u64(input, cursor)?,
        blanks: read_u64(input, cursor)?,
        definitions_only: read_u64(input, cursor)?,
        unsupported: read_u64(input, cursor)?,
        reference_definitions: read_u64(input, cursor)?,
        green_canonical_bytes: read_u64(input, cursor)?,
        projection_canonical_bytes: read_u64(input, cursor)?,
        green: decode_commitment(input, cursor)?,
        projection: decode_commitment(input, cursor)?,
    })
}

fn encode_commitment(
    commitment: LaneCommitment,
    output: &mut [u8],
    cursor: &mut usize,
) -> Result<(), M11BlockSequenceError> {
    for value in commitment.hash {
        write_u64(output, cursor, value)?;
    }
    for value in commitment.factor {
        write_u64(output, cursor, value)?;
    }
    Ok(())
}

fn decode_commitment(
    input: &[u8],
    cursor: &mut usize,
) -> Result<LaneCommitment, M11BlockSequenceError> {
    let mut hash = [0_u64; COMMITMENT_LANES];
    let mut factor = [0_u64; COMMITMENT_LANES];
    for value in &mut hash {
        *value = read_u64(input, cursor)?;
    }
    for value in &mut factor {
        *value = read_u64(input, cursor)?;
    }
    Ok(LaneCommitment { hash, factor })
}

const _: () = assert!(
    BLOCK_LEAF_HEADER_BYTES
        == 4 + 4 + 2 + 2 + 4 + (BLOCK_SUMMARY_COUNTERS * 8) + (2 * COMMITMENT_BYTES)
);
const _: () = assert!(
    BLOCK_BRANCH_BYTES == 4 + 4 + 8 + 2 + (BLOCK_SUMMARY_COUNTERS * 8) + (2 * COMMITMENT_BYTES)
);

/// Cumulative bounded work for one persistent block-sequence build.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11BlockSequenceBuildReceipt {
    transitions: usize,
    entries: u64,
    source_bytes: u64,
    source_utf16: u64,
    maximum_entry_bytes: usize,
    leaves_adopted: usize,
    branches_allocated: usize,
    branch_payload_bytes: usize,
    node_headers_decoded: u64,
    payload_bytes_inspected: u64,
    entries_hashed: u64,
    maximum_live_bins: usize,
    reserved_scratch_bytes: usize,
    seal_transitions: usize,
}

impl M11BlockSequenceBuildReceipt {
    fn from_mutation(
        transitions: usize,
        summary: BlockSequenceSummary,
        maximum_entry_bytes: usize,
        mutation: SequenceMutationReceipt,
        seal_transitions: usize,
    ) -> Self {
        Self {
            transitions,
            entries: summary.entries,
            source_bytes: summary.source_bytes,
            source_utf16: summary.source_utf16,
            maximum_entry_bytes,
            leaves_adopted: mutation.leaves_adopted,
            branches_allocated: mutation.branches_allocated,
            branch_payload_bytes: mutation.branch_payload_bytes,
            node_headers_decoded: mutation.inspection.node_headers_decoded,
            payload_bytes_inspected: mutation.inspection.spec.payload_bytes_inspected,
            entries_hashed: mutation.inspection.spec.spec_items_hashed,
            maximum_live_bins: mutation.maximum_live_bins,
            reserved_scratch_bytes: mutation.reserved_scratch_bytes,
            seal_transitions,
        }
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }

    #[must_use]
    pub const fn entries(self) -> u64 {
        self.entries
    }

    #[must_use]
    pub const fn source_bytes(self) -> u64 {
        self.source_bytes
    }

    #[must_use]
    pub const fn source_utf16(self) -> u64 {
        self.source_utf16
    }

    #[must_use]
    pub const fn maximum_entry_bytes(self) -> usize {
        self.maximum_entry_bytes
    }

    #[must_use]
    pub const fn leaves_adopted(self) -> usize {
        self.leaves_adopted
    }

    #[must_use]
    pub const fn branches_allocated(self) -> usize {
        self.branches_allocated
    }

    #[must_use]
    pub const fn branch_payload_bytes(self) -> usize {
        self.branch_payload_bytes
    }

    #[must_use]
    pub const fn node_headers_decoded(self) -> u64 {
        self.node_headers_decoded
    }

    #[must_use]
    pub const fn payload_bytes_inspected(self) -> u64 {
        self.payload_bytes_inspected
    }

    #[must_use]
    pub const fn entries_hashed(self) -> u64 {
        self.entries_hashed
    }

    #[must_use]
    pub const fn maximum_live_bins(self) -> usize {
        self.maximum_live_bins
    }

    #[must_use]
    pub const fn reserved_scratch_bytes(self) -> usize {
        self.reserved_scratch_bytes
    }

    #[must_use]
    pub const fn seal_transitions(self) -> usize {
        self.seal_transitions
    }
}

/// Exact structural work performed by one base-relative block splice.
///
/// Semantic entries are parser leaves. Storage leaves are bounded packed
/// arena pages. A splice may therefore re-encode at most the two storage pages
/// touching its semantic boundaries while retaining every untouched page and
/// path-copying only the measured-tree paths required by split/join.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11BlockSequenceSpliceReceipt {
    base_entries: u64,
    deleted_entries: u64,
    replacement_entries: u64,
    unchanged_entries_preserved: u64,
    boundary_entries_decoded: u64,
    boundary_entries_reencoded: u64,
    base_storage_pages: u64,
    deleted_storage_pages: u64,
    replacement_storage_pages: u64,
    reused_storage_pages: u64,
    node_headers_decoded: u64,
    summary_combinations: u64,
    payload_bytes_inspected: u64,
    entries_authenticated: u64,
    tree_nodes_visited: usize,
    branches_allocated: usize,
    branch_payload_bytes: usize,
    maximum_atomic_height: u16,
    seal_transitions: usize,
}

/// Parser-selected semantic ranges for one exact-base block splice.
///
/// The ranges are not authority by themselves. The producer revalidates the
/// base range against the retained exact publication and the target range
/// against the locally spliced target root before an exact stream may use
/// them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11BlockSequenceSpliceSelection {
    base_entry_range: Range<u64>,
    target_entry_range: Range<u64>,
}

impl M11BlockSequenceSpliceSelection {
    /// Binds two contiguous semantic ranges at the same sequence boundary.
    ///
    /// # Errors
    ///
    /// Returns [`M11BlockSequenceError::InvalidPoint`] for reversed ranges or
    /// ranges that do not start at the same semantic ordinal.
    pub fn new(
        base_entry_range: Range<u64>,
        target_entry_range: Range<u64>,
    ) -> Result<Self, M11BlockSequenceError> {
        if base_entry_range.start > base_entry_range.end
            || target_entry_range.start > target_entry_range.end
            || base_entry_range.start != target_entry_range.start
        {
            return Err(M11BlockSequenceError::InvalidPoint);
        }
        Ok(Self {
            base_entry_range,
            target_entry_range,
        })
    }

    #[must_use]
    pub fn base_entry_range(&self) -> Range<u64> {
        self.base_entry_range.clone()
    }

    #[must_use]
    pub fn target_entry_range(&self) -> Range<u64> {
        self.target_entry_range.clone()
    }
}

#[derive(Clone, Copy)]
struct BlockSequenceSpliceReceiptContext {
    base_entries: u64,
    deleted_entries: u64,
    replacement_entries: u64,
    boundary_entries_decoded: u64,
    boundary_entries_reencoded: u64,
    base_storage_pages: u64,
    deleted_storage_pages: u64,
    replacement_storage_pages: u64,
}

impl M11BlockSequenceSpliceReceipt {
    fn from_mutation(
        context: BlockSequenceSpliceReceiptContext,
        mutation: SequenceMutationReceipt,
        seal_transitions: usize,
    ) -> Result<Self, M11BlockSequenceError> {
        let observed_deleted_pages = u64::try_from(mutation.leaves_deleted)
            .map_err(|_| M11BlockSequenceError::CounterOverflow)?;
        let observed_reused_pages = u64::try_from(mutation.leaves_reused)
            .map_err(|_| M11BlockSequenceError::CounterOverflow)?;
        let observed_retained_pages = u64::try_from(mutation.committed_leaves_retained)
            .map_err(|_| M11BlockSequenceError::CounterOverflow)?;
        let expected_reused_pages = context
            .base_storage_pages
            .checked_sub(context.deleted_storage_pages)
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        if observed_deleted_pages != context.deleted_storage_pages
            || observed_reused_pages != expected_reused_pages
            || observed_retained_pages != context.base_storage_pages
            || u64::try_from(mutation.leaves_adopted)
                .map_err(|_| M11BlockSequenceError::CounterOverflow)?
                != context.replacement_storage_pages
        {
            return Err(M11BlockSequenceError::Corrupt(
                "block splice receipt differs from measured mutation work",
            ));
        }
        Ok(Self {
            base_entries: context.base_entries,
            deleted_entries: context.deleted_entries,
            replacement_entries: context.replacement_entries,
            unchanged_entries_preserved: context
                .base_entries
                .checked_sub(context.deleted_entries)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            boundary_entries_decoded: context.boundary_entries_decoded,
            boundary_entries_reencoded: context.boundary_entries_reencoded,
            base_storage_pages: context.base_storage_pages,
            deleted_storage_pages: context.deleted_storage_pages,
            replacement_storage_pages: context.replacement_storage_pages,
            reused_storage_pages: observed_reused_pages,
            node_headers_decoded: mutation.inspection.node_headers_decoded,
            summary_combinations: mutation.inspection.summary_combinations,
            payload_bytes_inspected: mutation.inspection.spec.payload_bytes_inspected,
            entries_authenticated: mutation.inspection.spec.spec_items_hashed,
            tree_nodes_visited: mutation.nodes_visited,
            branches_allocated: mutation.branches_allocated,
            branch_payload_bytes: mutation.branch_payload_bytes,
            maximum_atomic_height: mutation.maximum_atomic_height,
            seal_transitions,
        })
    }

    #[must_use]
    pub const fn base_entries(self) -> u64 {
        self.base_entries
    }

    #[must_use]
    pub const fn deleted_entries(self) -> u64 {
        self.deleted_entries
    }

    #[must_use]
    pub const fn replacement_entries(self) -> u64 {
        self.replacement_entries
    }

    #[must_use]
    pub const fn unchanged_entries_preserved(self) -> u64 {
        self.unchanged_entries_preserved
    }

    #[must_use]
    pub const fn boundary_entries_decoded(self) -> u64 {
        self.boundary_entries_decoded
    }

    #[must_use]
    pub const fn boundary_entries_reencoded(self) -> u64 {
        self.boundary_entries_reencoded
    }

    #[must_use]
    pub const fn base_storage_pages(self) -> u64 {
        self.base_storage_pages
    }

    #[must_use]
    pub const fn deleted_storage_pages(self) -> u64 {
        self.deleted_storage_pages
    }

    #[must_use]
    pub const fn replacement_storage_pages(self) -> u64 {
        self.replacement_storage_pages
    }

    #[must_use]
    pub const fn reused_storage_pages(self) -> u64 {
        self.reused_storage_pages
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

    #[must_use]
    pub const fn tree_nodes_visited(self) -> usize {
        self.tree_nodes_visited
    }

    #[must_use]
    pub const fn branches_allocated(self) -> usize {
        self.branches_allocated
    }

    #[must_use]
    pub const fn branch_payload_bytes(self) -> usize {
        self.branch_payload_bytes
    }

    #[must_use]
    pub const fn maximum_atomic_height(self) -> u16 {
        self.maximum_atomic_height
    }

    #[must_use]
    pub const fn seal_transitions(self) -> usize {
        self.seal_transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11BlockSequenceBuildStatus {
    NeedsInput,
    Pending,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11BlockSequenceBuildPoll {
    status: M11BlockSequenceBuildStatus,
    transitions: usize,
}

impl M11BlockSequenceBuildPoll {
    #[must_use]
    pub const fn status(self) -> M11BlockSequenceBuildStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuildPhase {
    Accepting,
    Pushing,
    ReadyForFinish,
    Finishing,
    ReadyForRoot,
    ReadyForSeal,
    Sealing,
    Complete,
    Cancelled,
    Failed,
}

/// Resumable builder for one exact-source block coverage sequence.
#[must_use = "block sequence builds require completion or explicit cancellation"]
pub struct M11BlockSequenceBuild {
    runtime_identity: RuntimeIdentity,
    lease: Option<SourceSnapshotLease>,
    source: SourceVersion,
    phase: BuildPhase,
    input_closed: bool,
    pending_entry: Option<M11BlockSequenceEntry>,
    page: [u8; ARENA_PAGE_BYTES],
    page_len: usize,
    page_entries: u16,
    page_summary: BlockSequenceSummary,
    builder: Option<BlockSequenceBuilder>,
    build: Option<CandidateBuild>,
    build_root: Option<BlockSequenceBuildRoot>,
    seal: Option<BlockSequenceSeal>,
    failed_tree: Option<BlockSequenceTree>,
    output: Option<M11BlockSequenceRoot>,
    mutation: SequenceMutationReceipt,
    transitions: usize,
    expected_summary: BlockSequenceSummary,
    maximum_entry_bytes: usize,
    seal_transitions: usize,
}

impl fmt::Debug for M11BlockSequenceBuild {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BlockSequenceBuild")
            .field("source", &self.source)
            .field("phase", &self.phase)
            .field("receipt", &self.receipt())
            .finish_non_exhaustive()
    }
}

impl M11BlockSequenceBuild {
    pub fn new(
        runtime: &DocumentRuntime,
        lease: SourceSnapshotLease,
    ) -> Result<Self, M11BlockSequenceError> {
        let source = lease.version();
        if runtime.current_source_version() != Some(source) {
            return Err(M11BlockSequenceError::SourceAuthorityMismatch);
        }
        Ok(Self {
            runtime_identity: runtime.producer_identity(),
            lease: Some(lease),
            source,
            phase: BuildPhase::Accepting,
            input_closed: false,
            pending_entry: None,
            page: [0; ARENA_PAGE_BYTES],
            page_len: BLOCK_LEAF_HEADER_BYTES,
            page_entries: 0,
            page_summary: BlockSequenceSummary::empty(),
            builder: None,
            build: None,
            build_root: None,
            seal: None,
            failed_tree: None,
            output: None,
            mutation: SequenceMutationReceipt::default(),
            transitions: 0,
            expected_summary: BlockSequenceSummary::empty(),
            maximum_entry_bytes: 0,
            seal_transitions: 0,
        })
    }

    pub fn offer_entry(
        &mut self,
        entry: M11BlockSequenceEntry,
    ) -> Result<(), M11BlockSequenceError> {
        if self.input_closed {
            return Err(M11BlockSequenceError::InputClosed);
        }
        if self.phase != BuildPhase::Accepting {
            return Err(M11BlockSequenceError::InvalidState);
        }
        if self.pending_entry.is_some() {
            return Err(M11BlockSequenceError::EntryAlreadyPending);
        }
        let next_bytes = self
            .expected_summary
            .source_bytes
            .checked_add(entry.source_bytes)
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        let next_utf16 = self
            .expected_summary
            .source_utf16
            .checked_add(entry.source_utf16)
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        if next_bytes
            > u64::try_from(self.source.byte_len())
                .map_err(|_| M11BlockSequenceError::CounterOverflow)?
            || next_utf16
                > u64::try_from(self.source.utf16_len())
                    .map_err(|_| M11BlockSequenceError::CounterOverflow)?
        {
            return Err(M11BlockSequenceError::InvalidEntryCoverage);
        }
        let next_byte_offset =
            usize::try_from(next_bytes).map_err(|_| M11BlockSequenceError::CounterOverflow)?;
        let actual_utf16 = self
            .lease
            .as_ref()
            .ok_or(M11BlockSequenceError::InvalidState)?
            .utf16_offset_for_byte(next_byte_offset)?;
        if u64::try_from(actual_utf16).map_err(|_| M11BlockSequenceError::CounterOverflow)?
            != next_utf16
        {
            return Err(M11BlockSequenceError::InvalidEntryCoverage);
        }
        self.pending_entry = Some(entry);
        Ok(())
    }

    pub fn finish_input(&mut self) -> Result<(), M11BlockSequenceError> {
        if self.input_closed {
            return Err(M11BlockSequenceError::InputClosed);
        }
        if self.phase != BuildPhase::Accepting || self.pending_entry.is_some() {
            return Err(M11BlockSequenceError::InvalidState);
        }
        if self.expected_summary.source_bytes
            != u64::try_from(self.source.byte_len())
                .map_err(|_| M11BlockSequenceError::CounterOverflow)?
            || self.expected_summary.source_utf16
                != u64::try_from(self.source.utf16_len())
                    .map_err(|_| M11BlockSequenceError::CounterOverflow)?
        {
            return Err(M11BlockSequenceError::IncompleteCoverage);
        }
        self.input_closed = true;
        Ok(())
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11BlockSequenceBuildPoll, M11BlockSequenceError> {
        self.ensure_runtime(runtime)?;
        validate_fuel(fuel)?;
        let before = self.transitions;
        while self.transitions - before < fuel {
            match self.phase {
                BuildPhase::Accepting if self.pending_entry.is_none() && !self.input_closed => {
                    return Ok(M11BlockSequenceBuildPoll {
                        status: M11BlockSequenceBuildStatus::NeedsInput,
                        transitions: self.transitions - before,
                    });
                }
                BuildPhase::Complete => {
                    return Ok(M11BlockSequenceBuildPoll {
                        status: M11BlockSequenceBuildStatus::Complete,
                        transitions: self.transitions - before,
                    });
                }
                BuildPhase::Cancelled => {
                    return Ok(M11BlockSequenceBuildPoll {
                        status: M11BlockSequenceBuildStatus::Cancelled,
                        transitions: self.transitions - before,
                    });
                }
                BuildPhase::Failed => return Err(M11BlockSequenceError::InvalidState),
                _ => self.step(runtime)?,
            }
            self.transitions = self
                .transitions
                .checked_add(1)
                .ok_or(M11BlockSequenceError::CounterOverflow)?;
        }
        Ok(M11BlockSequenceBuildPoll {
            status: match self.phase {
                BuildPhase::Complete => M11BlockSequenceBuildStatus::Complete,
                BuildPhase::Cancelled => M11BlockSequenceBuildStatus::Cancelled,
                BuildPhase::Accepting if self.pending_entry.is_none() && !self.input_closed => {
                    M11BlockSequenceBuildStatus::NeedsInput
                }
                _ => M11BlockSequenceBuildStatus::Pending,
            },
            transitions: self.transitions - before,
        })
    }

    fn step(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11BlockSequenceError> {
        match self.phase {
            BuildPhase::Accepting => self.step_accepting(runtime),
            BuildPhase::Pushing => self.poll_push(runtime),
            BuildPhase::ReadyForFinish => self.begin_finish(runtime),
            BuildPhase::Finishing => self.poll_finish(runtime),
            BuildPhase::ReadyForRoot => self.take_build_root(runtime),
            BuildPhase::ReadyForSeal => self.begin_seal(runtime),
            BuildPhase::Sealing => self.poll_seal(runtime),
            BuildPhase::Complete | BuildPhase::Cancelled | BuildPhase::Failed => {
                Err(M11BlockSequenceError::InvalidState)
            }
        }
    }

    fn step_accepting(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11BlockSequenceError> {
        if let Some(entry) = self.pending_entry.as_ref() {
            let encoded_len = entry.encoded_len();
            if self.page_entries > 0
                && (usize::from(self.page_entries) >= M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX
                    || self.page_len + encoded_len > ARENA_PAGE_BYTES)
            {
                return self.begin_page(runtime);
            }
            if self.page_len + encoded_len > ARENA_PAGE_BYTES {
                self.phase = BuildPhase::Failed;
                return Err(M11BlockSequenceError::RoleRecordTooLarge {
                    bytes: encoded_len,
                    cap: ARENA_PAGE_BYTES - BLOCK_LEAF_HEADER_BYTES,
                });
            }
            let next_page_summary = self
                .page_summary
                .checked_followed_by(entry_summary(entry))?;
            let next_expected_summary = self
                .expected_summary
                .checked_followed_by(entry_summary(entry))?;
            encode_entry(entry, &mut self.page, &mut self.page_len)?;
            self.page_entries = self
                .page_entries
                .checked_add(1)
                .ok_or(M11BlockSequenceError::CounterOverflow)?;
            self.page_summary = next_page_summary;
            self.expected_summary = next_expected_summary;
            self.maximum_entry_bytes = self.maximum_entry_bytes.max(encoded_len);
            self.pending_entry.take();
            return Ok(());
        }
        if !self.input_closed {
            return Err(M11BlockSequenceError::InvalidState);
        }
        if self.page_entries > 0 {
            return self.begin_page(runtime);
        }
        if self.builder.is_some() {
            self.phase = BuildPhase::ReadyForFinish;
            return Ok(());
        }
        let lease = self
            .lease
            .take()
            .ok_or(M11BlockSequenceError::InvalidState)?;
        self.output = Some(M11BlockSequenceRoot::empty(
            self.runtime_identity,
            lease,
            self.receipt(),
        ));
        self.phase = BuildPhase::Complete;
        Ok(())
    }

    fn begin_page(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11BlockSequenceError> {
        encode_leaf_header(
            &mut self.page,
            self.page_entries,
            self.page_len - BLOCK_LEAF_HEADER_BYTES,
            self.page_summary,
        )?;
        let payload_len = self.page_len;
        if self.builder.is_none() {
            let result = (|| {
                let mut session = runtime.producer_arena_mut().begin_build()?;
                let mut builder = BlockSequenceBuilder::try_new(&mut session, &mut self.mutation)?;
                let leaf = session.allocate(&self.page[..payload_len], &[])?;
                builder.begin_push(&session, leaf, &mut self.mutation)?;
                let build = session.suspend()?;
                Ok::<_, M11BlockSequenceError>((builder, build))
            })();
            match result {
                Ok((builder, build)) => {
                    self.builder = Some(builder);
                    self.build = Some(build);
                    self.reset_page();
                    self.phase = BuildPhase::Pushing;
                    return Ok(());
                }
                Err(error) => {
                    self.phase = BuildPhase::Failed;
                    return Err(error);
                }
            }
        }

        let build = self
            .build
            .as_ref()
            .ok_or(M11BlockSequenceError::InvalidState)?;
        runtime.producer_arena().validate_suspended_build(build)?;
        let build = self
            .build
            .take()
            .ok_or(M11BlockSequenceError::InvalidState)?;
        let result = (|| {
            let mut session = runtime.producer_arena_mut().resume_build(build)?;
            let leaf = session.allocate(&self.page[..payload_len], &[])?;
            self.builder
                .as_mut()
                .ok_or(M11BlockSequenceError::InvalidState)?
                .begin_push(&session, leaf, &mut self.mutation)?;
            Ok::<_, M11BlockSequenceError>(session.suspend()?)
        })();
        match result {
            Ok(build) => {
                self.build = Some(build);
                self.reset_page();
                self.phase = BuildPhase::Pushing;
                Ok(())
            }
            Err(error) => {
                self.builder = None;
                self.phase = BuildPhase::Failed;
                Err(error)
            }
        }
    }

    fn poll_push(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11BlockSequenceError> {
        let progress = self.with_resumed_build(runtime, |builder, session, mutation| {
            builder.poll_push(session, mutation)
        })?;
        self.phase = match progress {
            ResumableSequenceProgress::Pending => BuildPhase::Pushing,
            ResumableSequenceProgress::Complete if self.input_closed => BuildPhase::ReadyForFinish,
            ResumableSequenceProgress::Complete => BuildPhase::Accepting,
        };
        Ok(())
    }

    fn begin_finish(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11BlockSequenceError> {
        self.with_resumed_build(runtime, |builder, session, mutation| {
            builder.begin_finish(session, mutation)
        })?;
        self.phase = BuildPhase::Finishing;
        Ok(())
    }

    fn poll_finish(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11BlockSequenceError> {
        let progress = self.with_resumed_build(runtime, |builder, session, mutation| {
            builder.poll_finish(session, mutation)
        })?;
        self.phase = match progress {
            ResumableSequenceProgress::Pending => BuildPhase::Finishing,
            ResumableSequenceProgress::Complete => BuildPhase::ReadyForRoot,
        };
        Ok(())
    }

    fn take_build_root(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11BlockSequenceError> {
        let build = self
            .build
            .as_ref()
            .ok_or(M11BlockSequenceError::InvalidState)?;
        runtime.producer_arena().validate_suspended_build(build)?;
        let build = self
            .build
            .take()
            .ok_or(M11BlockSequenceError::InvalidState)?;
        let session = runtime.producer_arena_mut().resume_build(build)?;
        match self
            .builder
            .as_mut()
            .ok_or(M11BlockSequenceError::InvalidState)?
            .take_root(&session)
        {
            Ok(root) => match session.suspend() {
                Ok(build) => {
                    self.build = Some(build);
                    self.build_root = Some(root);
                    self.phase = BuildPhase::ReadyForSeal;
                    Ok(())
                }
                Err(error) => {
                    self.builder = None;
                    self.phase = BuildPhase::Failed;
                    Err(error.into())
                }
            },
            Err(error) => {
                drop(session);
                self.builder = None;
                self.phase = BuildPhase::Failed;
                Err(error)
            }
        }
    }

    fn begin_seal(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11BlockSequenceError> {
        let build = self
            .build
            .as_ref()
            .ok_or(M11BlockSequenceError::InvalidState)?;
        runtime.producer_arena().validate_suspended_build(build)?;
        let build = self
            .build
            .take()
            .ok_or(M11BlockSequenceError::InvalidState)?;
        let root = self
            .build_root
            .take()
            .ok_or(M11BlockSequenceError::InvalidState)?;
        match begin_measured_sequence_seal(runtime.producer_arena_mut(), build, root) {
            Ok(seal) => {
                self.builder = None;
                self.seal = Some(seal);
                self.phase = BuildPhase::Sealing;
                Ok(())
            }
            Err(BeginMeasuredSequenceSealFailure { error, build, root }) => {
                self.build = Some(build);
                self.build_root = Some(root);
                Err(error.into())
            }
        }
    }

    fn poll_seal(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11BlockSequenceError> {
        let poll = self
            .seal
            .as_mut()
            .ok_or(M11BlockSequenceError::InvalidState)?
            .poll(runtime.producer_arena_mut(), 1)?;
        self.seal_transitions = self
            .seal_transitions
            .checked_add(poll.transitions)
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        let Some(tree) = poll.root else {
            return Ok(());
        };
        self.seal = None;
        self.complete_nonempty(runtime, tree)
    }

    fn complete_nonempty(
        &mut self,
        runtime: &mut DocumentRuntime,
        tree: BlockSequenceTree,
    ) -> Result<(), M11BlockSequenceError> {
        let mut inspection = SequenceInspectionReceipt::default();
        let measure = match tree
            .as_ref()
            .summary(runtime.producer_arena(), &mut inspection)
        {
            Ok(Some(measure)) => measure,
            Ok(None) => {
                return self.reject_committed_tree(
                    runtime,
                    tree,
                    M11BlockSequenceError::Corrupt("nonempty block build sealed an empty root"),
                );
            }
            Err(error) => return self.reject_committed_tree(runtime, tree, error),
        };
        if let Err(error) = add_inspection(&mut self.mutation.inspection, inspection) {
            return self.reject_committed_tree(runtime, tree, error);
        }
        let expected_leaves = match u64::try_from(self.mutation.leaves_adopted) {
            Ok(leaves) => leaves,
            Err(_) => {
                return self.reject_committed_tree(
                    runtime,
                    tree,
                    M11BlockSequenceError::CounterOverflow,
                );
            }
        };
        if measure.summary() != self.expected_summary
            || measure.leaves() != expected_leaves
            || self.expected_summary.source_bytes
                != u64::try_from(self.source.byte_len())
                    .map_err(|_| M11BlockSequenceError::CounterOverflow)?
            || self.expected_summary.source_utf16
                != u64::try_from(self.source.utf16_len())
                    .map_err(|_| M11BlockSequenceError::CounterOverflow)?
            || self.lease.is_none()
        {
            return self.reject_committed_tree(
                runtime,
                tree,
                M11BlockSequenceError::Corrupt(
                    "sealed block summary differs from accepted source coverage",
                ),
            );
        }
        let lease = self
            .lease
            .take()
            .expect("validated block sequence source lease");
        self.output = Some(M11BlockSequenceRoot {
            runtime_identity: self.runtime_identity,
            lease: Some(lease),
            source: self.source,
            summary: measure.summary(),
            page_count: measure.leaves(),
            tree_height: measure.height(),
            tree: Some(tree),
            receipt: self.receipt(),
            released: false,
        });
        self.phase = BuildPhase::Complete;
        Ok(())
    }

    fn reject_committed_tree(
        &mut self,
        runtime: &mut DocumentRuntime,
        tree: BlockSequenceTree,
        error: M11BlockSequenceError,
    ) -> Result<(), M11BlockSequenceError> {
        self.phase = BuildPhase::Failed;
        match tree.release(runtime.producer_arena_mut()) {
            Ok(()) => Err(error),
            Err(failure) => {
                let release_error = failure.error;
                self.failed_tree = Some(failure.root);
                Err(release_error.into())
            }
        }
    }

    fn with_resumed_build<T>(
        &mut self,
        runtime: &mut DocumentRuntime,
        operation: impl FnOnce(
            &mut BlockSequenceBuilder,
            &mut ArenaBuildSession<'_>,
            &mut SequenceMutationReceipt,
        ) -> Result<T, M11BlockSequenceError>,
    ) -> Result<T, M11BlockSequenceError> {
        let build = self
            .build
            .as_ref()
            .ok_or(M11BlockSequenceError::InvalidState)?;
        runtime.producer_arena().validate_suspended_build(build)?;
        let build = self
            .build
            .take()
            .ok_or(M11BlockSequenceError::InvalidState)?;
        let mut session = runtime.producer_arena_mut().resume_build(build)?;
        let result = operation(
            self.builder
                .as_mut()
                .ok_or(M11BlockSequenceError::InvalidState)?,
            &mut session,
            &mut self.mutation,
        );
        match result {
            Ok(value) => {
                self.build = Some(session.suspend()?);
                Ok(value)
            }
            Err(error) => {
                drop(session);
                self.builder = None;
                self.phase = BuildPhase::Failed;
                Err(error)
            }
        }
    }

    fn reset_page(&mut self) {
        self.page.fill(0);
        self.page_len = BLOCK_LEAF_HEADER_BYTES;
        self.page_entries = 0;
        self.page_summary = BlockSequenceSummary::empty();
    }

    fn ensure_runtime(&self, runtime: &DocumentRuntime) -> Result<(), M11BlockSequenceError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11BlockSequenceError::WrongRuntime);
        }
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11BlockSequenceError::SourceAuthorityMismatch);
        }
        Ok(())
    }

    fn ensure_runtime_identity(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<(), M11BlockSequenceError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11BlockSequenceError::WrongRuntime);
        }
        Ok(())
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11BlockSequenceError> {
        self.ensure_runtime_identity(runtime)?;
        if let Some(mut output) = self.output.take() {
            if let Err(error) = output.begin_release(runtime) {
                self.output = Some(output);
                return Err(error);
            }
        }
        if let Some(tree) = self.failed_tree.take() {
            if let Err(failure) = tree.release(runtime.producer_arena_mut()) {
                let error = failure.error;
                self.failed_tree = Some(failure.root);
                return Err(error.into());
            }
        }
        if let Some(seal) = self.seal.take() {
            if let Err(failure) = seal.abort(runtime.producer_arena_mut()) {
                self.seal = Some(failure.seal);
                return Err(failure.error.into());
            }
        }
        if let Some(build) = self.build.as_ref() {
            runtime.producer_arena().validate_suspended_build(build)?;
        }
        if let Some(build) = self.build.take() {
            runtime.producer_arena_mut().abort_build(build)?;
        }
        self.builder = None;
        self.build_root = None;
        self.pending_entry = None;
        self.lease.take();
        self.phase = BuildPhase::Cancelled;
        Ok(())
    }

    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11BlockSequenceReclaimPoll, M11BlockSequenceError> {
        self.ensure_runtime_identity(runtime)?;
        if self.phase != BuildPhase::Cancelled {
            return Err(M11BlockSequenceError::InvalidState);
        }
        poll_reclaim(runtime, fuel)
    }

    #[must_use]
    pub fn take_root(&mut self) -> Option<M11BlockSequenceRoot> {
        if self.phase != BuildPhase::Complete {
            return None;
        }
        let receipt = self.receipt();
        if let Some(root) = self.output.as_mut() {
            root.receipt = receipt;
        }
        self.output.take()
    }

    #[must_use]
    pub fn receipt(&self) -> M11BlockSequenceBuildReceipt {
        M11BlockSequenceBuildReceipt::from_mutation(
            self.transitions,
            self.expected_summary,
            self.maximum_entry_bytes,
            self.mutation,
            self.seal_transitions,
        )
    }
}

impl Drop for M11BlockSequenceBuild {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.build.is_none()
                    && self.seal.is_none()
                    && self.failed_tree.is_none()
                    && self.output.is_none()
                    && self.lease.is_none(),
                "block sequence builds require root transfer or explicit cancellation"
            );
        }
    }
}

/// Exact scalar-aligned document point used for one block lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11BlockSequencePoint {
    byte_offset: usize,
    utf16_offset: usize,
    affinity: SourceBoundaryAffinity,
}

impl M11BlockSequencePoint {
    #[must_use]
    pub const fn new(
        byte_offset: usize,
        utf16_offset: usize,
        affinity: SourceBoundaryAffinity,
    ) -> Self {
        Self {
            byte_offset,
            utf16_offset,
            affinity,
        }
    }

    #[must_use]
    pub const fn byte_offset(self) -> usize {
        self.byte_offset
    }

    #[must_use]
    pub const fn utf16_offset(self) -> usize {
        self.utf16_offset
    }

    #[must_use]
    pub const fn affinity(self) -> SourceBoundaryAffinity {
        self.affinity
    }
}

/// Bounded inspection performed by one logarithmic point lookup.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11BlockSequenceQueryReceipt {
    node_headers_decoded: u64,
    summary_combinations: u64,
    payload_bytes_inspected: u64,
    entries_authenticated: u64,
    entries_scanned: u16,
}

impl M11BlockSequenceQueryReceipt {
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

    /// Entries decoded while authenticating the complete packed target page.
    ///
    /// This includes repeated schema-owned decodes performed by checked tree
    /// routing plus the final typed page decode.
    #[must_use]
    pub const fn entries_authenticated(self) -> u64 {
        self.entries_authenticated
    }

    /// Prefix entries examined after authentication to select the point.
    #[must_use]
    pub const fn entries_scanned(self) -> u16 {
        self.entries_scanned
    }
}

/// One selected semantic entry with its exact absolute dual-coordinate range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11BlockSequenceLocation {
    entry_ordinal: u64,
    storage_page_ordinal: u64,
    byte_range: Range<u64>,
    utf16_range: Range<u64>,
    entry: M11BlockSequenceEntry,
    receipt: M11BlockSequenceQueryReceipt,
}

impl M11BlockSequenceLocation {
    #[must_use]
    pub const fn entry_ordinal(&self) -> u64 {
        self.entry_ordinal
    }

    #[must_use]
    pub const fn storage_page_ordinal(&self) -> u64 {
        self.storage_page_ordinal
    }

    #[must_use]
    pub fn byte_range(&self) -> Range<u64> {
        self.byte_range.clone()
    }

    #[must_use]
    pub fn utf16_range(&self) -> Range<u64> {
        self.utf16_range.clone()
    }

    #[must_use]
    pub const fn entry(&self) -> &M11BlockSequenceEntry {
        &self.entry
    }

    #[must_use]
    pub const fn receipt(&self) -> M11BlockSequenceQueryReceipt {
        self.receipt
    }
}

/// Exact semantic resume point for one consecutive block visit.
///
/// The ordinal alone is not authority. Its independently carried dual-metric
/// cut must match the authenticated prefix of that ordinal before any entry is
/// yielded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11BlockSequenceVisitStart {
    pub(crate) entry_ordinal: u64,
    pub(crate) byte_offset: u64,
    pub(crate) utf16_offset: u64,
}

/// Whether a direct consecutive block visitor should continue after the
/// current semantic entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11BlockSequenceVisitControl {
    Continue,
    Stop,
}

/// One synchronously borrowed semantic entry in a consecutive block visit.
///
/// Storage-page identity is deliberately absent. Absolute source geometry and
/// semantic ordinal are the only continuation-relevant coordinates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct M11BlockSequenceVisitEntry<'entry> {
    entry_ordinal: u64,
    byte_start: u64,
    byte_end: u64,
    utf16_start: u64,
    utf16_end: u64,
    entry: &'entry M11BlockSequenceEntry,
}

impl<'entry> M11BlockSequenceVisitEntry<'entry> {
    pub(crate) const fn entry_ordinal(self) -> u64 {
        self.entry_ordinal
    }

    pub(crate) const fn byte_start(self) -> u64 {
        self.byte_start
    }

    pub(crate) const fn byte_end(self) -> u64 {
        self.byte_end
    }

    pub(crate) const fn utf16_start(self) -> u64 {
        self.utf16_start
    }

    pub(crate) const fn utf16_end(self) -> u64 {
        self.utf16_end
    }

    pub(crate) const fn entry(self) -> &'entry M11BlockSequenceEntry {
        self.entry
    }
}

/// Why one bounded consecutive block visit returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11BlockSequenceVisitDisposition {
    Complete,
    EntryLimit,
    StoragePageLimit,
    VisitorStopped,
}

/// Aggregate authenticated work and exact next semantic cut for one visit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11BlockSequenceVisitReceipt {
    visited_entries: u64,
    storage_pages_visited: u64,
    next_entry_ordinal: u64,
    next_byte_offset: u64,
    next_utf16_offset: u64,
    disposition: M11BlockSequenceVisitDisposition,
    inspection: SequenceInspectionReceipt,
}

impl M11BlockSequenceVisitReceipt {
    pub(crate) const fn visited_entries(self) -> u64 {
        self.visited_entries
    }

    pub(crate) const fn storage_pages_visited(self) -> u64 {
        self.storage_pages_visited
    }

    pub(crate) const fn next_entry_ordinal(self) -> u64 {
        self.next_entry_ordinal
    }

    pub(crate) const fn next_byte_offset(self) -> u64 {
        self.next_byte_offset
    }

    pub(crate) const fn next_utf16_offset(self) -> u64 {
        self.next_utf16_offset
    }

    pub(crate) const fn disposition(self) -> M11BlockSequenceVisitDisposition {
        self.disposition
    }

    pub(crate) const fn inspection(self) -> SequenceInspectionReceipt {
        self.inspection
    }
}

/// One exact half-open semantic ordinal window located directly in the
/// persistent measured block sequence.
///
/// The cuts are source-prefix metrics, not caller-authored coordinates. Blank
/// coverage remains a normal canonical entry and therefore participates in
/// both ordinals exactly like every other top-level structural boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11BlockSequenceOrdinalWindow {
    total_entry_count: u64,
    start_entry_ordinal: u64,
    next_entry_ordinal: u64,
    start_byte_offset: u64,
    start_utf16_offset: u64,
    next_byte_offset: u64,
    next_utf16_offset: u64,
    receipt: M11BlockSequenceOrdinalWindowReceipt,
}

impl M11BlockSequenceOrdinalWindow {
    pub(crate) const fn total_entry_count(self) -> u64 {
        self.total_entry_count
    }

    pub(crate) const fn start_entry_ordinal(self) -> u64 {
        self.start_entry_ordinal
    }

    pub(crate) const fn next_entry_ordinal(self) -> u64 {
        self.next_entry_ordinal
    }

    pub(crate) const fn start_byte_offset(self) -> u64 {
        self.start_byte_offset
    }

    pub(crate) const fn start_utf16_offset(self) -> u64 {
        self.start_utf16_offset
    }

    pub(crate) const fn next_byte_offset(self) -> u64 {
        self.next_byte_offset
    }

    pub(crate) const fn next_utf16_offset(self) -> u64 {
        self.next_utf16_offset
    }

    pub(crate) const fn complete(self) -> bool {
        self.next_entry_ordinal == self.total_entry_count
    }

    pub(crate) const fn receipt(self) -> M11BlockSequenceOrdinalWindowReceipt {
        self.receipt
    }
}

/// Authenticated work performed by one ordinal-window lookup.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct M11BlockSequenceOrdinalWindowReceipt {
    storage_pages_visited: u64,
    node_headers_decoded: u64,
    summary_combinations: u64,
    packed_entries_inspected: u64,
}

impl M11BlockSequenceOrdinalWindowReceipt {
    pub(crate) const fn storage_pages_visited(self) -> u64 {
        self.storage_pages_visited
    }

    pub(crate) const fn node_headers_decoded(self) -> u64 {
        self.node_headers_decoded
    }

    pub(crate) const fn summary_combinations(self) -> u64 {
        self.summary_combinations
    }

    pub(crate) const fn packed_entries_inspected(self) -> u64 {
        self.packed_entries_inspected
    }
}

/// Compact lane descriptor authenticated by one authority-bound role wrapper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentM11BlockRoleDescriptor {
    lane: M11BlockRoleLane,
    source_bytes: u64,
    source_utf16: u64,
    entries: u64,
    paragraphs: u64,
    structured: u64,
    blanks: u64,
    definitions_only: u64,
    unsupported: u64,
    reference_definitions: u64,
    storage_page_count: u64,
    tree_height: u16,
    canonical_bytes: u64,
    commitment: LaneCommitment,
}

impl PersistentM11BlockRoleDescriptor {
    pub(crate) const fn record_count(self) -> u64 {
        self.entries
    }

    pub(crate) const fn canonical_bytes(self) -> u64 {
        self.canonical_bytes
    }

    #[cfg(test)]
    pub(crate) fn commitment256(self) -> [u8; 32] {
        self.commitment.checksum()
    }

    fn common_matches(self, other: Self) -> bool {
        self.source_bytes == other.source_bytes
            && self.source_utf16 == other.source_utf16
            && self.entries == other.entries
            && self.paragraphs == other.paragraphs
            && self.structured == other.structured
            && self.blanks == other.blanks
            && self.definitions_only == other.definitions_only
            && self.unsupported == other.unsupported
            && self.reference_definitions == other.reference_definitions
            && self.storage_page_count == other.storage_page_count
            && self.tree_height == other.tree_height
    }
}

/// Installed, fully validated claim needed for direct host-arena point lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentM11BlockRootClaim {
    summary: BlockSequenceSummary,
    storage_page_count: u64,
    tree_height: u16,
}

impl PersistentM11BlockRootClaim {
    pub(crate) const fn source_bytes(self) -> u64 {
        self.summary.source_bytes
    }

    pub(crate) const fn source_utf16(self) -> u64 {
        self.summary.source_utf16
    }

    pub(crate) const fn entry_count(self) -> u64 {
        self.summary.entries
    }

    pub(crate) const fn reference_definition_count(self) -> u64 {
        self.summary.reference_definitions
    }

    pub(crate) const fn storage_page_count(self) -> u64 {
        self.storage_page_count
    }

    pub(crate) const fn tree_height(self) -> u16 {
        self.tree_height
    }

    pub(crate) const fn maximum_point_query_node_headers(self) -> u64 {
        maximum_metric_lookup_node_headers(self.tree_height)
    }
}

fn descriptor_for(
    summary: BlockSequenceSummary,
    storage_page_count: u64,
    tree_height: u16,
    lane: M11BlockRoleLane,
) -> PersistentM11BlockRoleDescriptor {
    let (canonical_bytes, commitment) = match lane {
        M11BlockRoleLane::Green => (summary.green_canonical_bytes, summary.green),
        M11BlockRoleLane::Projection => (summary.projection_canonical_bytes, summary.projection),
    };
    PersistentM11BlockRoleDescriptor {
        lane,
        source_bytes: summary.source_bytes,
        source_utf16: summary.source_utf16,
        entries: summary.entries,
        paragraphs: summary.paragraphs,
        structured: summary.structured,
        blanks: summary.blanks,
        definitions_only: summary.definitions_only,
        unsupported: summary.unsupported,
        reference_definitions: summary.reference_definitions,
        storage_page_count,
        tree_height,
        canonical_bytes,
        commitment,
    }
}

pub(crate) fn encode_persistent_m11_block_role_descriptor(
    descriptor: PersistentM11BlockRoleDescriptor,
) -> Result<[u8; PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES], M11BlockSequenceError> {
    validate_descriptor(descriptor)?;
    let mut output = [0_u8; PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES];
    let mut cursor = 0;
    write_bytes(&mut output, &mut cursor, &BLOCK_ROLE_DESCRIPTOR_MAGIC)?;
    write_u32(&mut output, &mut cursor, BLOCK_SEQUENCE_SCHEMA)?;
    write_u8(&mut output, &mut cursor, descriptor.lane as u8)?;
    write_bytes(&mut output, &mut cursor, &[0; 7])?;
    write_u64(&mut output, &mut cursor, descriptor.source_bytes)?;
    write_u64(&mut output, &mut cursor, descriptor.source_utf16)?;
    write_u64(&mut output, &mut cursor, descriptor.entries)?;
    write_u64(&mut output, &mut cursor, descriptor.paragraphs)?;
    write_u64(&mut output, &mut cursor, descriptor.structured)?;
    write_u64(&mut output, &mut cursor, descriptor.blanks)?;
    write_u64(&mut output, &mut cursor, descriptor.definitions_only)?;
    write_u64(&mut output, &mut cursor, descriptor.unsupported)?;
    write_u64(&mut output, &mut cursor, descriptor.reference_definitions)?;
    write_u64(&mut output, &mut cursor, descriptor.storage_page_count)?;
    write_u16(&mut output, &mut cursor, descriptor.tree_height)?;
    write_bytes(&mut output, &mut cursor, &[0; 6])?;
    write_u64(&mut output, &mut cursor, descriptor.canonical_bytes)?;
    encode_commitment(descriptor.commitment, &mut output, &mut cursor)?;
    if cursor != output.len() {
        return Err(M11BlockSequenceError::Corrupt(
            "block role descriptor encoding length changed",
        ));
    }
    Ok(output)
}

pub(crate) fn decode_persistent_m11_block_role_descriptor(
    input: &[u8],
    expected_lane: M11BlockRoleLane,
    source_bytes: u64,
    source_utf16: u64,
) -> Result<PersistentM11BlockRoleDescriptor, M11BlockSequenceError> {
    if input.len() != PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES
        || input.get(..4) != Some(BLOCK_ROLE_DESCRIPTOR_MAGIC.as_slice())
    {
        return Err(M11BlockSequenceError::Corrupt(
            "block role descriptor has the wrong shape",
        ));
    }
    let mut cursor = 4;
    if read_u32(input, &mut cursor)? != BLOCK_SEQUENCE_SCHEMA {
        return Err(M11BlockSequenceError::Corrupt(
            "block role descriptor schema is unsupported",
        ));
    }
    let lane = M11BlockRoleLane::decode(read_u8(input, &mut cursor)?)?;
    if lane != expected_lane || read_bytes(input, &mut cursor, 7)? != [0; 7] {
        return Err(M11BlockSequenceError::Corrupt(
            "block role descriptor lane changed",
        ));
    }
    let descriptor = PersistentM11BlockRoleDescriptor {
        lane,
        source_bytes: read_u64(input, &mut cursor)?,
        source_utf16: read_u64(input, &mut cursor)?,
        entries: read_u64(input, &mut cursor)?,
        paragraphs: read_u64(input, &mut cursor)?,
        structured: read_u64(input, &mut cursor)?,
        blanks: read_u64(input, &mut cursor)?,
        definitions_only: read_u64(input, &mut cursor)?,
        unsupported: read_u64(input, &mut cursor)?,
        reference_definitions: read_u64(input, &mut cursor)?,
        storage_page_count: read_u64(input, &mut cursor)?,
        tree_height: read_u16(input, &mut cursor)?,
        canonical_bytes: {
            if read_bytes(input, &mut cursor, 6)? != [0; 6] {
                return Err(M11BlockSequenceError::Corrupt(
                    "block role descriptor reserved bytes changed",
                ));
            }
            read_u64(input, &mut cursor)?
        },
        commitment: decode_commitment(input, &mut cursor)?,
    };
    if cursor != input.len()
        || descriptor.source_bytes != source_bytes
        || descriptor.source_utf16 != source_utf16
    {
        return Err(M11BlockSequenceError::Corrupt(
            "block role descriptor source changed",
        ));
    }
    validate_descriptor(descriptor)?;
    Ok(descriptor)
}

fn validate_descriptor(
    descriptor: PersistentM11BlockRoleDescriptor,
) -> Result<(), M11BlockSequenceError> {
    let summary = BlockSequenceSummary {
        source_bytes: descriptor.source_bytes,
        source_utf16: descriptor.source_utf16,
        entries: descriptor.entries,
        paragraphs: descriptor.paragraphs,
        structured: descriptor.structured,
        blanks: descriptor.blanks,
        definitions_only: descriptor.definitions_only,
        unsupported: descriptor.unsupported,
        reference_definitions: descriptor.reference_definitions,
        green_canonical_bytes: descriptor.canonical_bytes,
        projection_canonical_bytes: descriptor.canonical_bytes,
        green: descriptor.commitment,
        projection: descriptor.commitment,
    };
    validate_summary_counts(summary)?;
    if (descriptor.entries == 0)
        != (descriptor.storage_page_count == 0 && descriptor.tree_height == 0)
        || (descriptor.entries != 0
            && (descriptor.storage_page_count == 0 || descriptor.tree_height == 0))
    {
        return Err(M11BlockSequenceError::Corrupt(
            "block role descriptor tree shape is invalid",
        ));
    }
    Ok(())
}

pub(crate) fn validate_persistent_m11_block_root(
    arena: &PageArena,
    root: Option<ArenaId>,
    green: PersistentM11BlockRoleDescriptor,
    projection: PersistentM11BlockRoleDescriptor,
) -> Result<PersistentM11BlockRootClaim, M11BlockSequenceError> {
    if green.lane != M11BlockRoleLane::Green
        || projection.lane != M11BlockRoleLane::Projection
        || !green.common_matches(projection)
    {
        return Err(M11BlockSequenceError::Corrupt(
            "paired block role descriptors disagree",
        ));
    }
    let summary = BlockSequenceSummary {
        source_bytes: green.source_bytes,
        source_utf16: green.source_utf16,
        entries: green.entries,
        paragraphs: green.paragraphs,
        structured: green.structured,
        blanks: green.blanks,
        definitions_only: green.definitions_only,
        unsupported: green.unsupported,
        reference_definitions: green.reference_definitions,
        green_canonical_bytes: green.canonical_bytes,
        projection_canonical_bytes: projection.canonical_bytes,
        green: green.commitment,
        projection: projection.commitment,
    };
    validate_summary_counts(summary)?;
    let claim = PersistentM11BlockRootClaim {
        summary,
        storage_page_count: green.storage_page_count,
        tree_height: green.tree_height,
    };
    match root {
        None if summary == BlockSequenceSummary::empty()
            && claim.storage_page_count == 0
            && claim.tree_height == 0 =>
        {
            Ok(claim)
        }
        None => Err(M11BlockSequenceError::Corrupt(
            "nonempty block descriptor lost its root",
        )),
        Some(_) if summary.entries == 0 => Err(M11BlockSequenceError::Corrupt(
            "empty block descriptor owns a root",
        )),
        Some(root) => {
            let mut inspection = SequenceInspectionReceipt::default();
            let measure = crate::measured_sequence::validate_measured_sequence_node::<
                BlockSequenceSpec,
            >(arena, root, &mut inspection)?;
            if measure.summary() != summary
                || measure.leaves() != claim.storage_page_count
                || measure.height() != claim.tree_height
            {
                return Err(M11BlockSequenceError::Corrupt(
                    "block root differs from its paired descriptors",
                ));
            }
            Ok(claim)
        }
    }
}

pub(crate) fn is_m11_block_sequence_node_payload(payload: &[u8]) -> bool {
    matches!(
        payload.get(..4),
        Some(magic) if magic == BLOCK_LEAF_MAGIC || magic == BLOCK_BRANCH_MAGIC
    )
}

pub(crate) fn validate_imported_m11_block_sequence_node(
    arena: &PageArena,
    id: ArenaId,
) -> Result<(), M11BlockSequenceError> {
    if !is_m11_block_sequence_node_payload(arena.payload(id)?) {
        return Err(M11BlockSequenceError::Corrupt(
            "imported block node has the wrong payload kind",
        ));
    }
    let mut inspection = SequenceInspectionReceipt::default();
    let _ = crate::measured_sequence::validate_measured_sequence_node::<BlockSequenceSpec>(
        arena,
        id,
        &mut inspection,
    )?;
    Ok(())
}

/// Counts both logical role lanes packed into one canonical block leaf.
pub(crate) fn m11_block_sequence_canonical_record_count(payload: &[u8]) -> u32 {
    let mut inspection = SequenceSpecInspection::default();
    decode_leaf(payload, &mut inspection)
        .ok()
        .flatten()
        .and_then(|leaf| u32::from(leaf.entries).checked_mul(2))
        .unwrap_or(0)
}

/// One journalled retain of a persistent block root.
///
/// The retained owner is intentionally crate-private until the manifest owns
/// the paired Green/Projection wrapper special-case.
pub(crate) struct M11RetainedBlockSequenceRoot {
    owner: Option<ArenaBuildOwner>,
    summary: BlockSequenceSummary,
    page_count: u64,
    tree_height: u16,
    #[cfg(test)]
    inspection: SequenceInspectionReceipt,
}

impl M11RetainedBlockSequenceRoot {
    pub(crate) fn take_owner(&mut self) -> Option<ArenaBuildOwner> {
        self.owner.take()
    }

    #[cfg(test)]
    pub(crate) const fn entry_count(&self) -> u64 {
        self.summary.entries
    }

    #[cfg(test)]
    pub(crate) const fn page_count(&self) -> u64 {
        self.page_count
    }

    #[cfg(test)]
    pub(crate) const fn tree_height(&self) -> u16 {
        self.tree_height
    }

    #[cfg(test)]
    pub(crate) const fn inspection(&self) -> SequenceInspectionReceipt {
        self.inspection
    }

    pub(crate) fn descriptor(&self, lane: M11BlockRoleLane) -> PersistentM11BlockRoleDescriptor {
        descriptor_for(self.summary, self.page_count, self.tree_height, lane)
    }
}

fn locate_point_in_arena(
    arena: &PageArena,
    tree: MeasuredSequenceRef<'_, BlockSequenceSpec>,
    summary: BlockSequenceSummary,
    point: M11BlockSequencePoint,
) -> Result<Option<M11BlockSequenceLocation>, M11BlockSequenceError> {
    locate_byte_in_arena(
        arena,
        tree,
        summary,
        point.byte_offset,
        point.affinity,
        Some(point.utf16_offset),
    )
}

fn locate_byte_in_arena(
    arena: &PageArena,
    tree: MeasuredSequenceRef<'_, BlockSequenceSpec>,
    summary: BlockSequenceSummary,
    byte_offset: usize,
    affinity: SourceBoundaryAffinity,
    expected_utf16_offset: Option<usize>,
) -> Result<Option<M11BlockSequenceLocation>, M11BlockSequenceError> {
    if summary.source_bytes == 0 {
        if summary != BlockSequenceSummary::empty() || tree.root_id().is_some() {
            return Err(M11BlockSequenceError::Corrupt(
                "empty source owns nonempty block coverage",
            ));
        }
        return Ok(None);
    }
    let total_bytes = summary.source_bytes;
    let byte_position =
        u64::try_from(byte_offset).map_err(|_| M11BlockSequenceError::CounterOverflow)?;
    let probe = match affinity {
        SourceBoundaryAffinity::Before if byte_position > 0 => byte_position - 1,
        SourceBoundaryAffinity::Before => 0,
        SourceBoundaryAffinity::After if byte_position < total_bytes => byte_position,
        SourceBoundaryAffinity::After => total_bytes - 1,
    };
    let mut inspection = SequenceInspectionReceipt::default();
    let located = tree
        .locate_leaf_containing_metric(
            arena,
            probe,
            |leaf_summary| leaf_summary.source_bytes,
            &mut inspection,
        )?
        .ok_or(M11BlockSequenceError::Corrupt(
            "block point is absent from complete source coverage",
        ))?;
    let page_byte_start = located.prefix.map_or(0, |prefix| prefix.source_bytes);
    let page_utf16_start = located.prefix.map_or(0, |prefix| prefix.source_utf16);
    let local_probe = probe
        .checked_sub(page_byte_start)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    let payload = arena.payload(located.id)?;
    let mut local_inspection = SequenceSpecInspection::default();
    let leaf = decode_leaf(payload, &mut local_inspection)?.ok_or(
        M11BlockSequenceError::Corrupt("located block page uses a branch payload"),
    )?;
    if leaf.summary != located.summary {
        return Err(M11BlockSequenceError::Corrupt(
            "located block page summary changed",
        ));
    }
    let mut cursor = leaf.entries_start;
    let mut local_bytes = 0_u64;
    let mut local_utf16 = 0_u64;
    let prefix_entries = located.prefix.map_or(0, |prefix| prefix.entries);
    for local_ordinal in 0..leaf.entries {
        let entry = decode_entry(payload, &mut cursor)?;
        let byte_end = local_bytes
            .checked_add(entry.source_bytes)
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        let utf16_end = local_utf16
            .checked_add(entry.source_utf16)
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        if local_probe < byte_end {
            let byte_start = page_byte_start
                .checked_add(local_bytes)
                .ok_or(M11BlockSequenceError::CounterOverflow)?;
            let utf16_start = page_utf16_start
                .checked_add(local_utf16)
                .ok_or(M11BlockSequenceError::CounterOverflow)?;
            let absolute_utf16_end = page_utf16_start
                .checked_add(utf16_end)
                .ok_or(M11BlockSequenceError::CounterOverflow)?;
            if let Some(expected_utf16_offset) = expected_utf16_offset {
                let utf16_position = u64::try_from(expected_utf16_offset)
                    .map_err(|_| M11BlockSequenceError::InvalidPoint)?;
                let utf16_probe = match affinity {
                    SourceBoundaryAffinity::Before if utf16_position > 0 => utf16_position - 1,
                    SourceBoundaryAffinity::Before => 0,
                    SourceBoundaryAffinity::After if utf16_position < summary.source_utf16 => {
                        utf16_position
                    }
                    SourceBoundaryAffinity::After => summary.source_utf16 - 1,
                };
                if utf16_probe < utf16_start || utf16_probe >= absolute_utf16_end {
                    return Err(M11BlockSequenceError::InvalidPoint);
                }
            }
            let receipt = M11BlockSequenceQueryReceipt {
                node_headers_decoded: inspection.node_headers_decoded,
                summary_combinations: inspection.summary_combinations,
                payload_bytes_inspected: inspection
                    .spec
                    .payload_bytes_inspected
                    .checked_add(local_inspection.payload_bytes_inspected)
                    .ok_or(M11BlockSequenceError::CounterOverflow)?,
                entries_authenticated: inspection
                    .spec
                    .spec_items_hashed
                    .checked_add(local_inspection.spec_items_hashed)
                    .ok_or(M11BlockSequenceError::CounterOverflow)?,
                entries_scanned: local_ordinal
                    .checked_add(1)
                    .ok_or(M11BlockSequenceError::CounterOverflow)?,
            };
            return Ok(Some(M11BlockSequenceLocation {
                entry_ordinal: prefix_entries
                    .checked_add(u64::from(local_ordinal))
                    .ok_or(M11BlockSequenceError::CounterOverflow)?,
                storage_page_ordinal: located.ordinal,
                byte_range: byte_start
                    ..page_byte_start
                        .checked_add(byte_end)
                        .ok_or(M11BlockSequenceError::CounterOverflow)?,
                utf16_range: utf16_start..absolute_utf16_end,
                entry,
                receipt,
            }));
        }
        local_bytes = byte_end;
        local_utf16 = utf16_end;
    }
    if cursor != leaf.entries_end {
        return Err(M11BlockSequenceError::Corrupt(
            "block point scan did not consume its packed page",
        ));
    }
    Err(M11BlockSequenceError::Corrupt(
        "block point fell outside its measured page",
    ))
}

/// Direct point lookup over one already-installed, fully validated block root.
pub(crate) fn persistent_m11_block_locate_point(
    arena: &PageArena,
    root: Option<ArenaId>,
    claim: PersistentM11BlockRootClaim,
    point: M11BlockSequencePoint,
) -> Result<Option<M11BlockSequenceLocation>, M11BlockSequenceError> {
    let byte_offset =
        u64::try_from(point.byte_offset).map_err(|_| M11BlockSequenceError::InvalidPoint)?;
    let utf16_offset =
        u64::try_from(point.utf16_offset).map_err(|_| M11BlockSequenceError::InvalidPoint)?;
    if byte_offset > claim.source_bytes() || utf16_offset > claim.source_utf16() {
        return Err(M11BlockSequenceError::InvalidPoint);
    }
    if (claim.entry_count() == 0) != root.is_none() {
        return Err(M11BlockSequenceError::Corrupt(
            "installed block root changed shape",
        ));
    }
    locate_point_in_arena(
        arena,
        MeasuredSequenceRef::<BlockSequenceSpec>::from_imported_root(root),
        claim.summary,
        point,
    )
}

fn persistent_m11_block_ordinal_cut(
    arena: &PageArena,
    tree: MeasuredSequenceRef<'_, BlockSequenceSpec>,
    claim: PersistentM11BlockRootClaim,
    entry_ordinal: u64,
    inspection: &mut SequenceInspectionReceipt,
    storage_pages_visited: &mut u64,
    entries_scanned: &mut u64,
) -> Result<(u64, u64), M11BlockSequenceError> {
    if entry_ordinal == 0 {
        return Ok((0, 0));
    }
    if entry_ordinal == claim.entry_count() {
        return Ok((claim.source_bytes(), claim.source_utf16()));
    }
    let located = tree
        .locate_leaf_containing_metric(arena, entry_ordinal, |summary| summary.entries, inspection)?
        .ok_or(M11BlockSequenceError::Corrupt(
            "block ordinal cut is absent from complete coverage",
        ))?;
    *storage_pages_visited = storage_pages_visited
        .checked_add(1)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    let prefix_entries = located.prefix.map_or(0, |prefix| prefix.entries);
    let local_ordinal = entry_ordinal
        .checked_sub(prefix_entries)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    if local_ordinal >= located.summary.entries {
        return Err(M11BlockSequenceError::Corrupt(
            "block ordinal cut escaped its measured page",
        ));
    }
    let payload = arena.payload(located.id)?;
    let leaf = decode_leaf(payload, &mut inspection.spec)?.ok_or(
        M11BlockSequenceError::Corrupt("block ordinal cut reached a branch payload"),
    )?;
    if leaf.summary != located.summary {
        return Err(M11BlockSequenceError::Corrupt(
            "block ordinal cut page summary changed",
        ));
    }

    let mut cursor = leaf.entries_start;
    let mut local_bytes = 0_u64;
    let mut local_utf16 = 0_u64;
    for scanned in 0..local_ordinal {
        let entry = decode_entry(payload, &mut cursor)?;
        local_bytes = local_bytes
            .checked_add(entry.source_bytes)
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        local_utf16 = local_utf16
            .checked_add(entry.source_utf16)
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        *entries_scanned = entries_scanned
            .checked_add(1)
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        debug_assert!(scanned < u64::from(leaf.entries));
    }
    let page_byte_start = located.prefix.map_or(0, |prefix| prefix.source_bytes);
    let page_utf16_start = located.prefix.map_or(0, |prefix| prefix.source_utf16);
    Ok((
        page_byte_start
            .checked_add(local_bytes)
            .ok_or(M11BlockSequenceError::CounterOverflow)?,
        page_utf16_start
            .checked_add(local_utf16)
            .ok_or(M11BlockSequenceError::CounterOverflow)?,
    ))
}

/// Locates an ordinal-bounded top-level structural window without walking its
/// source prefix.
///
/// At most two semantic boundary lookups are required. Each performs one
/// logarithmic measured-tree seek and authenticates one bounded packed page;
/// the number of intervening blocks does not affect lookup work.
pub(crate) fn persistent_m11_block_locate_ordinal_window(
    arena: &PageArena,
    root: Option<ArenaId>,
    claim: PersistentM11BlockRootClaim,
    start_entry_ordinal: u64,
    maximum_entries: u32,
) -> Result<M11BlockSequenceOrdinalWindow, M11BlockSequenceError> {
    if maximum_entries == 0
        || start_entry_ordinal > claim.entry_count()
        || (claim.entry_count() == 0) != root.is_none()
    {
        return Err(M11BlockSequenceError::InvalidPoint);
    }
    let next_entry_ordinal = start_entry_ordinal
        .saturating_add(u64::from(maximum_entries))
        .min(claim.entry_count());
    let tree = MeasuredSequenceRef::<BlockSequenceSpec>::from_imported_root(root);
    let mut inspection = SequenceInspectionReceipt::default();
    let mut storage_pages_visited = 0_u64;
    let mut entries_scanned = 0_u64;
    let (start_byte_offset, start_utf16_offset) = persistent_m11_block_ordinal_cut(
        arena,
        tree,
        claim,
        start_entry_ordinal,
        &mut inspection,
        &mut storage_pages_visited,
        &mut entries_scanned,
    )?;
    let (next_byte_offset, next_utf16_offset) = persistent_m11_block_ordinal_cut(
        arena,
        tree,
        claim,
        next_entry_ordinal,
        &mut inspection,
        &mut storage_pages_visited,
        &mut entries_scanned,
    )?;
    if start_byte_offset > next_byte_offset
        || start_utf16_offset > next_utf16_offset
        || (start_entry_ordinal == next_entry_ordinal)
            != (start_byte_offset == next_byte_offset && start_utf16_offset == next_utf16_offset)
        || (next_entry_ordinal == claim.entry_count())
            != (next_byte_offset == claim.source_bytes()
                && next_utf16_offset == claim.source_utf16())
    {
        return Err(M11BlockSequenceError::Corrupt(
            "block ordinal window cuts disagree with measured coverage",
        ));
    }
    let packed_entries_inspected = inspection
        .spec
        .spec_items_hashed
        .checked_add(entries_scanned)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    Ok(M11BlockSequenceOrdinalWindow {
        total_entry_count: claim.entry_count(),
        start_entry_ordinal,
        next_entry_ordinal,
        start_byte_offset,
        start_utf16_offset,
        next_byte_offset,
        next_utf16_offset,
        receipt: M11BlockSequenceOrdinalWindowReceipt {
            storage_pages_visited,
            node_headers_decoded: inspection.node_headers_decoded,
            summary_combinations: inspection.summary_combinations,
            packed_entries_inspected,
        },
    })
}

/// Visits consecutive semantic block entries from one authenticated resume
/// point without retaining or exposing a storage cursor.
///
/// The measured sequence performs one logarithmic metric seek and then walks
/// consecutive packed leaves directly. Before the first callback, the
/// semantic ordinal's measured prefix must equal both expected source cuts.
/// Every callback is synchronous and receives exact absolute dual-coordinate
/// geometry. `maximum_entries` bounds yielded semantic work independently of
/// packed-page topology.
pub(crate) fn persistent_m11_block_visit_entries(
    arena: &PageArena,
    root: Option<ArenaId>,
    claim: PersistentM11BlockRootClaim,
    start: M11BlockSequenceVisitStart,
    maximum_entries: u32,
    maximum_storage_pages: u32,
    mut visitor: impl FnMut(M11BlockSequenceVisitEntry<'_>) -> M11BlockSequenceVisitControl,
) -> Result<M11BlockSequenceVisitReceipt, M11BlockSequenceError> {
    if maximum_entries == 0
        || maximum_storage_pages == 0
        || start.entry_ordinal > claim.entry_count()
        || start.byte_offset > claim.source_bytes()
        || start.utf16_offset > claim.source_utf16()
        || (claim.entry_count() == 0) != root.is_none()
    {
        return Err(M11BlockSequenceError::InvalidPoint);
    }
    if start.entry_ordinal == claim.entry_count() {
        if start.byte_offset != claim.source_bytes() || start.utf16_offset != claim.source_utf16() {
            return Err(M11BlockSequenceError::InvalidPoint);
        }
        return Ok(M11BlockSequenceVisitReceipt {
            visited_entries: 0,
            storage_pages_visited: 0,
            next_entry_ordinal: start.entry_ordinal,
            next_byte_offset: start.byte_offset,
            next_utf16_offset: start.utf16_offset,
            disposition: M11BlockSequenceVisitDisposition::Complete,
            inspection: SequenceInspectionReceipt::default(),
        });
    }
    let root = root.ok_or(M11BlockSequenceError::Corrupt(
        "nonterminal block visit lost its measured root",
    ))?;
    let tree = MeasuredSequenceRef::<BlockSequenceSpec>::from_imported_root(Some(root));
    let mut tree_inspection = SequenceInspectionReceipt::default();
    let mut leaf_inspection = SequenceSpecInspection::default();
    let mut visited_entries = 0_u64;
    let mut storage_pages_visited = 0_u64;
    let mut next_entry_ordinal = start.entry_ordinal;
    let mut next_byte_offset = start.byte_offset;
    let mut next_utf16_offset = start.utf16_offset;
    let mut disposition = M11BlockSequenceVisitDisposition::Complete;
    let leaf_visit = tree.visit_leaves_from_metric(
        arena,
        start.entry_ordinal,
        |summary| summary.entries,
        &mut tree_inspection,
        |located| {
            storage_pages_visited = storage_pages_visited
                .checked_add(1)
                .ok_or(M11BlockSequenceError::CounterOverflow)?;
            let payload = arena.payload(located.id)?;
            let leaf = decode_leaf(payload, &mut leaf_inspection)?.ok_or(
                M11BlockSequenceError::Corrupt("ordered block visit reached a branch payload"),
            )?;
            if leaf.summary != located.summary {
                return Err(M11BlockSequenceError::Corrupt(
                    "ordered block visit page summary changed",
                ));
            }
            let page_entry_start = located.prefix.map_or(0, |prefix| prefix.entries);
            let page_byte_start = located.prefix.map_or(0, |prefix| prefix.source_bytes);
            let page_utf16_start = located.prefix.map_or(0, |prefix| prefix.source_utf16);
            let page_entry_end = page_entry_start
                .checked_add(u64::from(leaf.entries))
                .ok_or(M11BlockSequenceError::CounterOverflow)?;
            if next_entry_ordinal < page_entry_start || next_entry_ordinal >= page_entry_end {
                return Err(M11BlockSequenceError::Corrupt(
                    "ordered block visit escaped its measured page",
                ));
            }
            let local_start = u16::try_from(next_entry_ordinal - page_entry_start)
                .map_err(|_| M11BlockSequenceError::CounterOverflow)?;
            let mut cursor = leaf.entries_start;
            let mut local_bytes = 0_u64;
            let mut local_utf16 = 0_u64;
            for local_ordinal in 0..leaf.entries {
                let entry = decode_entry(payload, &mut cursor)?;
                let byte_end = local_bytes
                    .checked_add(entry.source_bytes)
                    .ok_or(M11BlockSequenceError::CounterOverflow)?;
                let utf16_end = local_utf16
                    .checked_add(entry.source_utf16)
                    .ok_or(M11BlockSequenceError::CounterOverflow)?;
                if local_ordinal >= local_start {
                    let byte_start = page_byte_start
                        .checked_add(local_bytes)
                        .ok_or(M11BlockSequenceError::CounterOverflow)?;
                    let utf16_start = page_utf16_start
                        .checked_add(local_utf16)
                        .ok_or(M11BlockSequenceError::CounterOverflow)?;
                    if byte_start != next_byte_offset || utf16_start != next_utf16_offset {
                        return Err(M11BlockSequenceError::InvalidPoint);
                    }
                    let absolute_byte_end = page_byte_start
                        .checked_add(byte_end)
                        .ok_or(M11BlockSequenceError::CounterOverflow)?;
                    let absolute_utf16_end = page_utf16_start
                        .checked_add(utf16_end)
                        .ok_or(M11BlockSequenceError::CounterOverflow)?;
                    let control = visitor(M11BlockSequenceVisitEntry {
                        entry_ordinal: next_entry_ordinal,
                        byte_start,
                        byte_end: absolute_byte_end,
                        utf16_start,
                        utf16_end: absolute_utf16_end,
                        entry: &entry,
                    });
                    visited_entries = visited_entries
                        .checked_add(1)
                        .ok_or(M11BlockSequenceError::CounterOverflow)?;
                    next_entry_ordinal = next_entry_ordinal
                        .checked_add(1)
                        .ok_or(M11BlockSequenceError::CounterOverflow)?;
                    next_byte_offset = absolute_byte_end;
                    next_utf16_offset = absolute_utf16_end;
                    if control == M11BlockSequenceVisitControl::Stop {
                        disposition = M11BlockSequenceVisitDisposition::VisitorStopped;
                        return Ok(SequenceLeafVisitControl::Stop);
                    }
                    if visited_entries == u64::from(maximum_entries) {
                        disposition = if next_entry_ordinal == claim.entry_count() {
                            M11BlockSequenceVisitDisposition::Complete
                        } else {
                            M11BlockSequenceVisitDisposition::EntryLimit
                        };
                        return Ok(SequenceLeafVisitControl::Stop);
                    }
                }
                local_bytes = byte_end;
                local_utf16 = utf16_end;
            }
            if cursor != leaf.entries_end
                || local_bytes != leaf.summary.source_bytes
                || local_utf16 != leaf.summary.source_utf16
            {
                return Err(M11BlockSequenceError::Corrupt(
                    "ordered block visit did not consume its packed page",
                ));
            }
            if next_entry_ordinal < claim.entry_count()
                && storage_pages_visited == u64::from(maximum_storage_pages)
            {
                disposition = M11BlockSequenceVisitDisposition::StoragePageLimit;
                return Ok(SequenceLeafVisitControl::Stop);
            }
            Ok(SequenceLeafVisitControl::Continue)
        },
    )?;
    if !leaf_visit.visitor_stopped() {
        if next_entry_ordinal != claim.entry_count()
            || next_byte_offset != claim.source_bytes()
            || next_utf16_offset != claim.source_utf16()
        {
            return Err(M11BlockSequenceError::Corrupt(
                "completed block visit did not reach exact source end",
            ));
        }
        disposition = M11BlockSequenceVisitDisposition::Complete;
    }
    if leaf_visit.leaves_visited() == 0 || visited_entries == 0 {
        return Err(M11BlockSequenceError::Corrupt(
            "nonterminal block visit made no semantic progress",
        ));
    }
    tree_inspection.spec.payload_bytes_inspected = tree_inspection
        .spec
        .payload_bytes_inspected
        .checked_add(leaf_inspection.payload_bytes_inspected)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    tree_inspection.spec.spec_items_hashed = tree_inspection
        .spec
        .spec_items_hashed
        .checked_add(leaf_inspection.spec_items_hashed)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    Ok(M11BlockSequenceVisitReceipt {
        visited_entries,
        storage_pages_visited,
        next_entry_ordinal,
        next_byte_offset,
        next_utf16_offset,
        disposition,
        inspection: tree_inspection,
    })
}

/// Exact-base byte routing over an authenticated persistent block root.
///
/// This is deliberately narrower than the public same-revision point query:
/// it accepts no caller-authored UTF-16 coordinate and derives the selected
/// entry's complete dual-coordinate range from the measured base itself.
/// Exact prefix/suffix lineage still has to authenticate any subsequent reuse
/// cut.
pub(crate) fn persistent_m11_block_locate_byte(
    arena: &PageArena,
    root: Option<ArenaId>,
    claim: PersistentM11BlockRootClaim,
    byte_offset: usize,
    affinity: SourceBoundaryAffinity,
) -> Result<Option<M11BlockSequenceLocation>, M11BlockSequenceError> {
    let byte_offset_u64 =
        u64::try_from(byte_offset).map_err(|_| M11BlockSequenceError::InvalidPoint)?;
    if byte_offset_u64 > claim.source_bytes() {
        return Err(M11BlockSequenceError::InvalidPoint);
    }
    if (claim.entry_count() == 0) != root.is_none() {
        return Err(M11BlockSequenceError::Corrupt(
            "installed block root changed shape",
        ));
    }
    locate_byte_in_arena(
        arena,
        MeasuredSequenceRef::<BlockSequenceSpec>::from_imported_root(root),
        claim.summary,
        byte_offset,
        affinity,
        None,
    )
}

/// Immutable, exact-source persistent block coverage.
#[must_use = "block sequence roots require explicit release"]
pub struct M11BlockSequenceRoot {
    runtime_identity: RuntimeIdentity,
    lease: Option<SourceSnapshotLease>,
    source: SourceVersion,
    summary: BlockSequenceSummary,
    page_count: u64,
    tree_height: u16,
    tree: Option<BlockSequenceTree>,
    receipt: M11BlockSequenceBuildReceipt,
    released: bool,
}

impl fmt::Debug for M11BlockSequenceRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BlockSequenceRoot")
            .field("source", &self.source)
            .field("entries", &self.summary.entries)
            .field("paragraphs", &self.summary.paragraphs)
            .field("structured", &self.summary.structured)
            .field("blanks", &self.summary.blanks)
            .field("definitions_only", &self.summary.definitions_only)
            .field("unsupported", &self.summary.unsupported)
            .field("page_count", &self.page_count)
            .field("tree_height", &self.tree_height)
            .finish_non_exhaustive()
    }
}

impl M11BlockSequenceRoot {
    fn empty(
        runtime_identity: RuntimeIdentity,
        lease: SourceSnapshotLease,
        receipt: M11BlockSequenceBuildReceipt,
    ) -> Self {
        let source = lease.version();
        Self {
            runtime_identity,
            lease: Some(lease),
            source,
            summary: BlockSequenceSummary::empty(),
            page_count: 0,
            tree_height: 0,
            tree: None,
            receipt,
            released: false,
        }
    }

    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn source_byte_len(&self) -> u64 {
        self.summary.source_bytes
    }

    #[must_use]
    pub const fn source_utf16_len(&self) -> u64 {
        self.summary.source_utf16
    }

    #[must_use]
    pub const fn entry_count(&self) -> u64 {
        self.summary.entries
    }

    #[must_use]
    pub const fn paragraph_count(&self) -> u64 {
        self.summary.paragraphs
    }

    #[must_use]
    pub const fn structured_count(&self) -> u64 {
        self.summary.structured
    }

    #[must_use]
    pub const fn blank_count(&self) -> u64 {
        self.summary.blanks
    }

    #[must_use]
    pub const fn definitions_only_count(&self) -> u64 {
        self.summary.definitions_only
    }

    #[must_use]
    pub const fn unsupported_count(&self) -> u64 {
        self.summary.unsupported
    }

    #[must_use]
    pub const fn reference_definition_count(&self) -> u64 {
        self.summary.reference_definitions
    }

    #[must_use]
    pub const fn storage_page_count(&self) -> u64 {
        self.page_count
    }

    #[must_use]
    pub const fn tree_height(&self) -> u16 {
        self.tree_height
    }

    #[must_use]
    pub fn green_commitment256(&self) -> [u8; 32] {
        self.summary.green.checksum()
    }

    #[must_use]
    pub fn projection_commitment256(&self) -> [u8; 32] {
        self.summary.projection.checksum()
    }

    #[must_use]
    pub const fn build_receipt(&self) -> M11BlockSequenceBuildReceipt {
        self.receipt
    }

    #[cfg(test)]
    pub(crate) fn tree_root_id_for_test(&self) -> Option<ArenaId> {
        self.tree
            .as_ref()
            .and_then(BlockSequenceTree::root_id_for_test)
    }

    /// Finds the semantic coverage selected by one byte/UTF-16 point.
    ///
    /// The AVL descent routes by summed UTF-8 bytes. Its authenticated prefix
    /// also supplies the absolute UTF-16 start; at most one bounded packed
    /// page is then scanned. `Before` selects the byte immediately before an
    /// interior boundary, while `After` selects the byte at that boundary.
    /// BOF and EOF clamp to the first and last coverage entry respectively.
    pub fn locate_point(
        &self,
        runtime: &DocumentRuntime,
        point: M11BlockSequencePoint,
    ) -> Result<Option<M11BlockSequenceLocation>, M11BlockSequenceError> {
        self.ensure_runtime(runtime)?;
        if self.released {
            return Err(M11BlockSequenceError::InvalidState);
        }
        if point.byte_offset > self.source.byte_len()
            || point.utf16_offset > self.source.utf16_len()
        {
            return Err(M11BlockSequenceError::InvalidPoint);
        }
        let actual_utf16 = self
            .lease
            .as_ref()
            .ok_or(M11BlockSequenceError::InvalidState)?
            .utf16_offset_for_byte(point.byte_offset)
            .map_err(|_| M11BlockSequenceError::InvalidPoint)?;
        if actual_utf16 != point.utf16_offset {
            return Err(M11BlockSequenceError::InvalidPoint);
        }
        if self.summary.source_bytes == 0 {
            if self.summary != BlockSequenceSummary::empty() || self.tree.is_some() {
                return Err(M11BlockSequenceError::Corrupt(
                    "empty source owns nonempty block coverage",
                ));
            }
            return Ok(None);
        }
        let tree = self.tree.as_ref().ok_or(M11BlockSequenceError::Corrupt(
            "nonempty block coverage lost its tree",
        ))?;
        locate_point_in_arena(runtime.producer_arena(), tree.as_ref(), self.summary, point)
    }

    pub(crate) fn retain_for_publication(
        &self,
        session: &mut ArenaBuildSession<'_>,
        expected_runtime_identity: RuntimeIdentity,
        expected_source: SourceVersion,
    ) -> Result<M11RetainedBlockSequenceRoot, M11BlockSequenceError> {
        if self.released
            || self.lease.is_none()
            || self.runtime_identity != expected_runtime_identity
            || self.source != expected_source
        {
            return Err(M11BlockSequenceError::SourceAuthorityMismatch);
        }
        let mut mutation = SequenceMutationReceipt::default();
        let owner = match self.tree.as_ref() {
            Some(tree) => {
                let (retained, measure) = retain_committed_measured_sequence_root_with_measure(
                    session,
                    tree,
                    &mut mutation,
                )?;
                if measure.summary() != self.summary
                    || measure.leaves() != self.page_count
                    || measure.height() != self.tree_height
                {
                    return Err(M11BlockSequenceError::Corrupt(
                        "retained block root summary changed",
                    ));
                }
                Some(retained.into_owner())
            }
            None => {
                if self.summary != BlockSequenceSummary::empty()
                    || self.page_count != 0
                    || self.tree_height != 0
                {
                    return Err(M11BlockSequenceError::Corrupt(
                        "empty block root changed shape",
                    ));
                }
                None
            }
        };
        Ok(M11RetainedBlockSequenceRoot {
            owner,
            summary: self.summary,
            page_count: self.page_count,
            tree_height: self.tree_height,
            #[cfg(test)]
            inspection: mutation.inspection,
        })
    }

    fn ensure_runtime(&self, runtime: &DocumentRuntime) -> Result<(), M11BlockSequenceError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11BlockSequenceError::WrongRuntime);
        }
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11BlockSequenceError::SourceAuthorityMismatch);
        }
        Ok(())
    }

    pub fn begin_release(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11BlockSequenceError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11BlockSequenceError::WrongRuntime);
        }
        if self.released {
            return Err(M11BlockSequenceError::InvalidState);
        }
        if let Some(tree) = self.tree.take() {
            match tree.release(runtime.producer_arena_mut()) {
                Ok(()) => {}
                Err(failure) => {
                    self.tree = Some(failure.root);
                    return Err(failure.error.into());
                }
            }
        }
        self.lease.take();
        self.released = true;
        Ok(())
    }

    pub fn poll_release(
        &self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11BlockSequenceReclaimPoll, M11BlockSequenceError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11BlockSequenceError::WrongRuntime);
        }
        if !self.released {
            return Err(M11BlockSequenceError::InvalidState);
        }
        poll_reclaim(runtime, fuel)
    }
}

struct BlockSemanticSplicePlan {
    storage_page_range: Range<u64>,
    prefix_entries: Vec<M11BlockSequenceEntry>,
    suffix_entries: Vec<M11BlockSequenceEntry>,
    boundary_entries_decoded: u64,
    inspection: SequenceInspectionReceipt,
}

/// Typed storage cut derived from one authenticated semantic block range.
///
/// The wire protocol carries this cut only as a claim. An independent host
/// recomputes it from its installed root before admitting replacement pages,
/// so a producer cannot redirect a semantic edit to an unrelated subtree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11BlockSequenceSemanticSplicePlan {
    pub(crate) storage_page_range: Range<u64>,
    pub(crate) prefix_entries: u64,
    pub(crate) suffix_entries: u64,
    pub(crate) boundary_entries_decoded: u64,
    inspection: SequenceInspectionReceipt,
}

/// Derives the bounded packed-page cut for a semantic block-entry range.
///
/// `claim` must have come from paired Green/Projection descriptor
/// validation. The result exposes no producer ownership and is safe to encode
/// as a replay claim because the host derives the same value independently.
pub(crate) fn plan_persistent_m11_block_semantic_splice(
    arena: &PageArena,
    root: Option<ArenaId>,
    claim: PersistentM11BlockRootClaim,
    entry_range: Range<u64>,
) -> Result<M11BlockSequenceSemanticSplicePlan, M11BlockSequenceError> {
    let plan = plan_block_semantic_splice(
        arena,
        MeasuredSequenceRef::from_imported_root(root),
        claim.summary,
        claim.storage_page_count,
        entry_range,
    )?;
    Ok(M11BlockSequenceSemanticSplicePlan {
        storage_page_range: plan.storage_page_range,
        prefix_entries: u64::try_from(plan.prefix_entries.len())
            .map_err(|_| M11BlockSequenceError::CounterOverflow)?,
        suffix_entries: u64::try_from(plan.suffix_entries.len())
            .map_err(|_| M11BlockSequenceError::CounterOverflow)?,
        boundary_entries_decoded: plan.boundary_entries_decoded,
        inspection: plan.inspection,
    })
}

/// Returns one authenticated packed block leaf by storage-page ordinal.
pub(crate) fn persistent_m11_block_storage_page_at(
    arena: &PageArena,
    root: Option<ArenaId>,
    ordinal: u64,
) -> Result<Option<&[u8]>, M11BlockSequenceError> {
    let Some(root) = root else {
        return Ok(None);
    };
    let mut inspection = SequenceInspectionReceipt::default();
    let sequence = MeasuredSequenceRef::<BlockSequenceSpec>::from_imported_root(Some(root));
    let Some(located) = sequence.locate_leaf_with_prefix(arena, ordinal, &mut inspection)? else {
        return Ok(None);
    };
    let payload = arena.payload(located.id)?;
    let leaf = decode_leaf(payload, &mut inspection.spec)?.ok_or(
        M11BlockSequenceError::Corrupt("located block storage page is not a leaf"),
    )?;
    if leaf.summary != located.summary || arena.child_count(located.id)? != 0 {
        return Err(M11BlockSequenceError::Corrupt(
            "located block storage page changed shape",
        ));
    }
    Ok(Some(payload))
}

/// Authenticated host replay claim for one semantic block splice.
///
/// Both entry ranges are semantic: `base_entry_range` names the entries
/// removed from the acknowledged base and `target_entry_range` names only the
/// parser-produced replacement entries in the target. The storage ranges name
/// the packed pages after boundary survivors have been included.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11BlockSequenceHostSpliceClaim {
    pub(crate) base_entry_range: Range<u64>,
    pub(crate) target_entry_range: Range<u64>,
    pub(crate) base_storage_range: Range<u64>,
    pub(crate) target_storage_range: Range<u64>,
    pub(crate) base_green: PersistentM11BlockRoleDescriptor,
    pub(crate) base_projection: PersistentM11BlockRoleDescriptor,
    pub(crate) target_green: PersistentM11BlockRoleDescriptor,
    pub(crate) target_projection: PersistentM11BlockRoleDescriptor,
}

/// Bounded work receipt for one independently replayed block splice.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct M11BlockSequenceHostSpliceWork {
    base_entries: u64,
    deleted_entries: u64,
    replacement_entries: u64,
    base_storage_pages: u64,
    transferred_storage_pages: u64,
    reused_storage_pages: u64,
    transferred_payload_bytes: u64,
    boundary_entries_decoded: u64,
    node_headers_decoded: u64,
    tree_nodes_visited: usize,
    branches_allocated: usize,
    branch_payload_bytes: usize,
    maximum_atomic_height: u16,
}

#[cfg(test)]
impl M11BlockSequenceHostSpliceWork {
    pub(crate) const fn base_entries(self) -> u64 {
        self.base_entries
    }
    pub(crate) const fn deleted_entries(self) -> u64 {
        self.deleted_entries
    }
    pub(crate) const fn replacement_entries(self) -> u64 {
        self.replacement_entries
    }
    pub(crate) const fn base_storage_pages(self) -> u64 {
        self.base_storage_pages
    }
    pub(crate) const fn transferred_storage_pages(self) -> u64 {
        self.transferred_storage_pages
    }
    pub(crate) const fn reused_storage_pages(self) -> u64 {
        self.reused_storage_pages
    }
    pub(crate) const fn transferred_payload_bytes(self) -> u64 {
        self.transferred_payload_bytes
    }
    pub(crate) const fn boundary_entries_decoded(self) -> u64 {
        self.boundary_entries_decoded
    }
    pub(crate) const fn node_headers_decoded(self) -> u64 {
        self.node_headers_decoded
    }
    pub(crate) const fn tree_nodes_visited(self) -> usize {
        self.tree_nodes_visited
    }
    pub(crate) const fn branches_allocated(self) -> usize {
        self.branches_allocated
    }
    pub(crate) const fn branch_payload_bytes(self) -> usize {
        self.branch_payload_bytes
    }
    pub(crate) const fn maximum_atomic_height(self) -> u16 {
        self.maximum_atomic_height
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11BlockSequenceHostReplayPoll {
    Pending,
    Complete,
}

pub(crate) struct M11BlockSequenceHostReplayOutput {
    root: Option<ArenaBuildOwner>,
    work: M11BlockSequenceHostSpliceWork,
}

impl M11BlockSequenceHostReplayOutput {
    pub(crate) fn into_parts(self) -> (Option<ArenaBuildOwner>, M11BlockSequenceHostSpliceWork) {
        (self.root, self.work)
    }
}

enum M11BlockSequenceHostReplayPhase {
    Accepting {
        builder: Option<BlockSequenceBuilder>,
        push_active: bool,
    },
    Finishing(BlockSequenceBuilder),
    Ready {
        replacement: Option<BlockSequenceBuildRoot>,
    },
    Complete,
    Poisoned,
}

/// Abort-journal-safe replay of one exact base-relative block splice.
///
/// The caller retains the installed block root into one host build journal,
/// allocates only the replacement `BSL1` pages, and transfers every owner to
/// this typed state machine. The acknowledged base is authenticated again,
/// the semantic range is independently mapped to packed pages, and the final
/// root must exactly match both target lane commitments before it is released
/// to a manifest wrapper.
pub(crate) struct M11BlockSequenceHostReplay {
    base: Option<BlockSequenceBuildRoot>,
    claim: M11BlockSequenceHostSpliceClaim,
    plan: M11BlockSequenceSemanticSplicePlan,
    target_claim: PersistentM11BlockRootClaim,
    replacement_page_count: u64,
    replacement_summary: BlockSequenceSummary,
    replacement_payload_bytes: u64,
    phase: M11BlockSequenceHostReplayPhase,
    base_validation_receipt: SequenceMutationReceipt,
    replacement_receipt: SequenceMutationReceipt,
    splice_receipt: SequenceMutationReceipt,
    target_validation_receipt: SequenceMutationReceipt,
}

impl M11BlockSequenceHostReplay {
    pub(crate) fn new(
        session: &ArenaBuildSession<'_>,
        base_owner: Option<ArenaBuildOwner>,
        claim: M11BlockSequenceHostSpliceClaim,
    ) -> Result<Self, M11BlockSequenceError> {
        if claim.base_entry_range.start > claim.base_entry_range.end
            || claim.target_entry_range.start > claim.target_entry_range.end
            || claim.base_entry_range.start != claim.target_entry_range.start
            || claim.base_storage_range.start > claim.base_storage_range.end
            || claim.target_storage_range.start > claim.target_storage_range.end
            || claim.base_storage_range.start != claim.target_storage_range.start
        {
            return Err(M11BlockSequenceError::InvalidPoint);
        }

        let base_root = base_owner.as_ref().map(ArenaBuildOwner::id);
        let base_claim =
            paired_m11_block_descriptor_claim(claim.base_green, claim.base_projection)?;
        let target_claim =
            paired_m11_block_descriptor_claim(claim.target_green, claim.target_projection)?;
        let deleted_entries = claim.base_entry_range.end - claim.base_entry_range.start;
        let replacement_entries = claim.target_entry_range.end - claim.target_entry_range.start;
        let expected_target_entries = base_claim
            .entry_count()
            .checked_sub(deleted_entries)
            .and_then(|entries| entries.checked_add(replacement_entries))
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        if claim.base_entry_range.end > base_claim.entry_count()
            || claim.target_entry_range.end > target_claim.entry_count()
            || target_claim.entry_count() != expected_target_entries
        {
            return Err(M11BlockSequenceError::InvalidPoint);
        }

        let plan = plan_persistent_m11_block_semantic_splice(
            session.arena(),
            base_root,
            base_claim,
            claim.base_entry_range.clone(),
        )?;
        if plan.storage_page_range != claim.base_storage_range {
            return Err(M11BlockSequenceError::Corrupt(
                "block replay storage cut differs from its semantic range",
            ));
        }
        let removed_pages = claim.base_storage_range.end - claim.base_storage_range.start;
        let replacement_pages = claim.target_storage_range.end - claim.target_storage_range.start;
        let expected_target_pages = base_claim
            .storage_page_count()
            .checked_sub(removed_pages)
            .and_then(|pages| pages.checked_add(replacement_pages))
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        if claim.base_storage_range.end > base_claim.storage_page_count()
            || claim.target_storage_range.end > target_claim.storage_page_count()
            || target_claim.storage_page_count() != expected_target_pages
        {
            return Err(M11BlockSequenceError::Corrupt(
                "block replay storage-page arithmetic changed",
            ));
        }

        let mut base_validation_receipt = SequenceMutationReceipt::default();
        let base = match base_owner {
            Some(owner) => {
                session.validate_owner(&owner)?;
                let measure =
                    crate::measured_sequence::validate_measured_sequence_node::<BlockSequenceSpec>(
                        session.arena(),
                        owner.id(),
                        &mut base_validation_receipt.inspection,
                    )?;
                if measure.summary() != base_claim.summary
                    || measure.leaves() != base_claim.storage_page_count
                    || measure.height() != base_claim.tree_height
                {
                    return Err(M11BlockSequenceError::Corrupt(
                        "block replay base differs from its paired descriptors",
                    ));
                }
                Some(validate_measured_sequence_build_owner::<BlockSequenceSpec>(
                    session,
                    owner,
                    &mut base_validation_receipt,
                )?)
            }
            None => {
                if base_claim.entry_count() != 0 {
                    return Err(M11BlockSequenceError::Corrupt(
                        "nonempty block replay base lost its owner",
                    ));
                }
                None
            }
        };
        Ok(Self {
            base,
            claim,
            plan,
            target_claim,
            replacement_page_count: 0,
            replacement_summary: BlockSequenceSummary::empty(),
            replacement_payload_bytes: 0,
            phase: M11BlockSequenceHostReplayPhase::Accepting {
                builder: None,
                push_active: false,
            },
            base_validation_receipt,
            replacement_receipt: SequenceMutationReceipt::default(),
            splice_receipt: SequenceMutationReceipt::default(),
            target_validation_receipt: SequenceMutationReceipt::default(),
        })
    }

    pub(crate) fn offer_replacement_leaf(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        leaf: ArenaBuildOwner,
    ) -> Result<(), M11BlockSequenceError> {
        let phase = std::mem::replace(&mut self.phase, M11BlockSequenceHostReplayPhase::Poisoned);
        let M11BlockSequenceHostReplayPhase::Accepting {
            builder,
            push_active: false,
        } = phase
        else {
            return Err(M11BlockSequenceError::InvalidState);
        };
        if self.replacement_page_count
            >= self.claim.target_storage_range.end - self.claim.target_storage_range.start
        {
            return Err(M11BlockSequenceError::Corrupt(
                "too many block replacement pages",
            ));
        }
        session.validate_owner(&leaf)?;
        if session.arena().child_count(leaf.id())? != 0 {
            return Err(M11BlockSequenceError::Corrupt(
                "block replacement page owns children",
            ));
        }
        let payload = session.arena().payload(leaf.id())?;
        let mut inspection = SequenceSpecInspection::default();
        let decoded = decode_leaf(payload, &mut inspection)?.ok_or(
            M11BlockSequenceError::Corrupt("block replacement page is not a leaf"),
        )?;
        self.replacement_summary = self
            .replacement_summary
            .checked_followed_by(decoded.summary)?;
        self.replacement_payload_bytes = self
            .replacement_payload_bytes
            .checked_add(
                u64::try_from(payload.len()).map_err(|_| M11BlockSequenceError::CounterOverflow)?,
            )
            .ok_or(M11BlockSequenceError::CounterOverflow)?;

        let mut builder = match builder {
            Some(builder) => builder,
            None => BlockSequenceBuilder::try_new(session, &mut self.replacement_receipt)?,
        };
        builder.begin_push(session, leaf, &mut self.replacement_receipt)?;
        self.replacement_page_count = self
            .replacement_page_count
            .checked_add(1)
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        self.phase = M11BlockSequenceHostReplayPhase::Accepting {
            builder: Some(builder),
            push_active: true,
        };
        Ok(())
    }

    pub(crate) fn poll_replacement(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<M11BlockSequenceHostReplayPoll, M11BlockSequenceError> {
        let phase = std::mem::replace(&mut self.phase, M11BlockSequenceHostReplayPhase::Poisoned);
        match phase {
            M11BlockSequenceHostReplayPhase::Accepting {
                builder: Some(mut builder),
                push_active: true,
            } => {
                let progress = builder.poll_push(session, &mut self.replacement_receipt)?;
                self.phase = M11BlockSequenceHostReplayPhase::Accepting {
                    builder: Some(builder),
                    push_active: progress != ResumableSequenceProgress::Complete,
                };
                Ok(if progress == ResumableSequenceProgress::Complete {
                    M11BlockSequenceHostReplayPoll::Complete
                } else {
                    M11BlockSequenceHostReplayPoll::Pending
                })
            }
            M11BlockSequenceHostReplayPhase::Finishing(mut builder) => {
                let progress = builder.poll_finish(session, &mut self.replacement_receipt)?;
                if progress == ResumableSequenceProgress::Complete {
                    let replacement = builder.take_root(session)?;
                    self.phase = M11BlockSequenceHostReplayPhase::Ready {
                        replacement: Some(replacement),
                    };
                    Ok(M11BlockSequenceHostReplayPoll::Complete)
                } else {
                    self.phase = M11BlockSequenceHostReplayPhase::Finishing(builder);
                    Ok(M11BlockSequenceHostReplayPoll::Pending)
                }
            }
            _ => Err(M11BlockSequenceError::InvalidState),
        }
    }

    pub(crate) fn finish_replacement(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<M11BlockSequenceHostReplayPoll, M11BlockSequenceError> {
        let phase = std::mem::replace(&mut self.phase, M11BlockSequenceHostReplayPhase::Poisoned);
        let M11BlockSequenceHostReplayPhase::Accepting {
            builder,
            push_active: false,
        } = phase
        else {
            return Err(M11BlockSequenceError::InvalidState);
        };
        let expected_pages =
            self.claim.target_storage_range.end - self.claim.target_storage_range.start;
        let expected_entries = self
            .plan
            .prefix_entries
            .checked_add(self.claim.target_entry_range.end - self.claim.target_entry_range.start)
            .and_then(|entries| entries.checked_add(self.plan.suffix_entries))
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        if self.replacement_page_count != expected_pages
            || self.replacement_summary.entries != expected_entries
        {
            return Err(M11BlockSequenceError::Corrupt(
                "block replacement pages differ from the semantic splice",
            ));
        }
        match builder {
            Some(mut builder) => {
                builder.begin_finish(session, &mut self.replacement_receipt)?;
                self.phase = M11BlockSequenceHostReplayPhase::Finishing(builder);
                Ok(M11BlockSequenceHostReplayPoll::Pending)
            }
            None => {
                self.phase = M11BlockSequenceHostReplayPhase::Ready { replacement: None };
                Ok(M11BlockSequenceHostReplayPoll::Complete)
            }
        }
    }

    pub(crate) fn complete(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<M11BlockSequenceHostReplayOutput, M11BlockSequenceError> {
        let phase = std::mem::replace(&mut self.phase, M11BlockSequenceHostReplayPhase::Poisoned);
        let M11BlockSequenceHostReplayPhase::Ready { replacement } = phase else {
            return Err(M11BlockSequenceError::InvalidState);
        };
        let target = match self.base.take() {
            Some(base) => splice_measured_sequence_build_root_atomic::<BlockSequenceSpec>(
                session,
                base,
                self.claim.base_storage_range.clone(),
                replacement,
                &mut self.splice_receipt,
            )?,
            None => {
                if self.claim.base_storage_range != (0..0) {
                    return Err(M11BlockSequenceError::Corrupt(
                        "empty block replay base changed its cut",
                    ));
                }
                replacement
            }
        };
        let target_owner = target.map(MeasuredSequenceBuildRoot::into_owner);
        if let Some(owner) = target_owner.as_ref() {
            session.validate_owner(owner)?;
            let measure =
                crate::measured_sequence::validate_measured_sequence_node::<BlockSequenceSpec>(
                    session.arena(),
                    owner.id(),
                    &mut self.target_validation_receipt.inspection,
                )?;
            if measure.summary() != self.target_claim.summary
                || measure.leaves() != self.target_claim.storage_page_count
                || measure.height() != self.target_claim.tree_height
            {
                return Err(M11BlockSequenceError::Corrupt(
                    "block replay target claim changed after splice",
                ));
            }
        } else if self.target_claim.entry_count() != 0 {
            return Err(M11BlockSequenceError::Corrupt(
                "nonempty block replay target lost its root",
            ));
        }
        let deleted_entries = self.claim.base_entry_range.end - self.claim.base_entry_range.start;
        let replacement_entries =
            self.claim.target_entry_range.end - self.claim.target_entry_range.start;
        let deleted_pages = self.claim.base_storage_range.end - self.claim.base_storage_range.start;
        let reused_storage_pages = self
            .claim
            .base_green
            .storage_page_count
            .checked_sub(deleted_pages)
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        let work = M11BlockSequenceHostSpliceWork {
            base_entries: self.claim.base_green.entries,
            deleted_entries,
            replacement_entries,
            base_storage_pages: self.claim.base_green.storage_page_count,
            transferred_storage_pages: self.replacement_page_count,
            reused_storage_pages,
            transferred_payload_bytes: self.replacement_payload_bytes,
            boundary_entries_decoded: self.plan.boundary_entries_decoded,
            node_headers_decoded: self
                .base_validation_receipt
                .inspection
                .node_headers_decoded
                .checked_add(self.plan.inspection.node_headers_decoded)
                .and_then(|headers| {
                    headers.checked_add(self.replacement_receipt.inspection.node_headers_decoded)
                })
                .and_then(|headers| {
                    headers.checked_add(self.splice_receipt.inspection.node_headers_decoded)
                })
                .and_then(|headers| {
                    headers.checked_add(
                        self.target_validation_receipt
                            .inspection
                            .node_headers_decoded,
                    )
                })
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            tree_nodes_visited: self.splice_receipt.nodes_visited,
            branches_allocated: self
                .replacement_receipt
                .branches_allocated
                .checked_add(self.splice_receipt.branches_allocated)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            branch_payload_bytes: self
                .replacement_receipt
                .branch_payload_bytes
                .checked_add(self.splice_receipt.branch_payload_bytes)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
            maximum_atomic_height: self
                .replacement_receipt
                .maximum_atomic_height
                .max(self.splice_receipt.maximum_atomic_height),
        };
        self.phase = M11BlockSequenceHostReplayPhase::Complete;
        Ok(M11BlockSequenceHostReplayOutput {
            root: target_owner,
            work,
        })
    }
}

fn paired_m11_block_descriptor_claim(
    green: PersistentM11BlockRoleDescriptor,
    projection: PersistentM11BlockRoleDescriptor,
) -> Result<PersistentM11BlockRootClaim, M11BlockSequenceError> {
    if green.lane != M11BlockRoleLane::Green
        || projection.lane != M11BlockRoleLane::Projection
        || !green.common_matches(projection)
    {
        return Err(M11BlockSequenceError::Corrupt(
            "paired target block descriptors disagree",
        ));
    }
    validate_descriptor(green)?;
    validate_descriptor(projection)?;
    Ok(PersistentM11BlockRootClaim {
        summary: BlockSequenceSummary {
            source_bytes: green.source_bytes,
            source_utf16: green.source_utf16,
            entries: green.entries,
            paragraphs: green.paragraphs,
            structured: green.structured,
            blanks: green.blanks,
            definitions_only: green.definitions_only,
            unsupported: green.unsupported,
            reference_definitions: green.reference_definitions,
            green_canonical_bytes: green.canonical_bytes,
            projection_canonical_bytes: projection.canonical_bytes,
            green: green.commitment,
            projection: projection.commitment,
        },
        storage_page_count: green.storage_page_count,
        tree_height: green.tree_height,
    })
}

fn decode_block_storage_page(
    arena: &PageArena,
    id: ArenaId,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<Vec<M11BlockSequenceEntry>, M11BlockSequenceError> {
    let payload = arena.payload(id)?;
    let leaf = decode_leaf(payload, &mut inspection.spec)?.ok_or(
        M11BlockSequenceError::Corrupt("block splice boundary uses a branch payload"),
    )?;
    let mut entries = Vec::with_capacity(usize::from(leaf.entries));
    let mut cursor = leaf.entries_start;
    for _ in 0..leaf.entries {
        entries.push(decode_entry(payload, &mut cursor)?);
    }
    if cursor != leaf.entries_end {
        return Err(M11BlockSequenceError::Corrupt(
            "block splice boundary did not consume its packed page",
        ));
    }
    Ok(entries)
}

fn plan_block_semantic_splice(
    arena: &PageArena,
    sequence: MeasuredSequenceRef<'_, BlockSequenceSpec>,
    summary: BlockSequenceSummary,
    page_count: u64,
    entry_range: Range<u64>,
) -> Result<BlockSemanticSplicePlan, M11BlockSequenceError> {
    if entry_range.start > entry_range.end || entry_range.end > summary.entries {
        return Err(M11BlockSequenceError::InvalidPoint);
    }
    if summary.entries == 0 {
        if sequence.root_id().is_some() || page_count != 0 || !entry_range.is_empty() {
            return Err(M11BlockSequenceError::Corrupt(
                "empty block splice base changed shape",
            ));
        }
        return Ok(BlockSemanticSplicePlan {
            storage_page_range: 0..0,
            prefix_entries: Vec::new(),
            suffix_entries: Vec::new(),
            boundary_entries_decoded: 0,
            inspection: SequenceInspectionReceipt::default(),
        });
    }
    if sequence.root_id().is_none() {
        return Err(M11BlockSequenceError::Corrupt(
            "nonempty block splice base lost its tree",
        ));
    }
    let mut inspection = SequenceInspectionReceipt::default();

    // Appending at semantic EOF does not disturb the final packed page.
    if entry_range.is_empty() && entry_range.start == summary.entries {
        return Ok(BlockSemanticSplicePlan {
            storage_page_range: page_count..page_count,
            prefix_entries: Vec::new(),
            suffix_entries: Vec::new(),
            boundary_entries_decoded: 0,
            inspection,
        });
    }

    let first = sequence
        .locate_leaf_containing_metric(
            arena,
            entry_range.start,
            |value| value.entries,
            &mut inspection,
        )?
        .ok_or(M11BlockSequenceError::Corrupt(
            "block splice start is absent from complete coverage",
        ))?;
    let first_prefix_entries = first.prefix.map_or(0, |prefix| prefix.entries);
    let first_local_start = entry_range
        .start
        .checked_sub(first_prefix_entries)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    if first_local_start >= first.summary.entries {
        return Err(M11BlockSequenceError::Corrupt(
            "block splice start escaped its packed page",
        ));
    }
    let first_entries = decode_block_storage_page(arena, first.id, &mut inspection)?;
    let first_local_start =
        usize::try_from(first_local_start).map_err(|_| M11BlockSequenceError::CounterOverflow)?;

    if entry_range.is_empty() {
        return Ok(BlockSemanticSplicePlan {
            storage_page_range: first.ordinal
                ..first
                    .ordinal
                    .checked_add(1)
                    .ok_or(M11BlockSequenceError::CounterOverflow)?,
            prefix_entries: first_entries[..first_local_start].to_vec(),
            suffix_entries: first_entries[first_local_start..].to_vec(),
            boundary_entries_decoded: u64::try_from(first_entries.len())
                .map_err(|_| M11BlockSequenceError::CounterOverflow)?,
            inspection,
        });
    }

    let first_page_end = first_prefix_entries
        .checked_add(first.summary.entries)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    if entry_range.end <= first_page_end {
        let local_end = usize::try_from(
            entry_range
                .end
                .checked_sub(first_prefix_entries)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
        )
        .map_err(|_| M11BlockSequenceError::CounterOverflow)?;
        return Ok(BlockSemanticSplicePlan {
            storage_page_range: first.ordinal
                ..first
                    .ordinal
                    .checked_add(1)
                    .ok_or(M11BlockSequenceError::CounterOverflow)?,
            prefix_entries: first_entries[..first_local_start].to_vec(),
            suffix_entries: first_entries[local_end..].to_vec(),
            boundary_entries_decoded: u64::try_from(first_entries.len())
                .map_err(|_| M11BlockSequenceError::CounterOverflow)?,
            inspection,
        });
    }

    let last_probe = entry_range
        .end
        .checked_sub(1)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    let last = sequence
        .locate_leaf_containing_metric(arena, last_probe, |value| value.entries, &mut inspection)?
        .ok_or(M11BlockSequenceError::Corrupt(
            "block splice end is absent from complete coverage",
        ))?;
    let last_prefix_entries = last.prefix.map_or(0, |prefix| prefix.entries);
    let last_local_end = usize::try_from(
        entry_range
            .end
            .checked_sub(last_prefix_entries)
            .ok_or(M11BlockSequenceError::CounterOverflow)?,
    )
    .map_err(|_| M11BlockSequenceError::CounterOverflow)?;
    let last_entries = decode_block_storage_page(arena, last.id, &mut inspection)?;
    if last_local_end == 0 || last_local_end > last_entries.len() {
        return Err(M11BlockSequenceError::Corrupt(
            "block splice end escaped its packed page",
        ));
    }
    Ok(BlockSemanticSplicePlan {
        storage_page_range: first.ordinal
            ..last
                .ordinal
                .checked_add(1)
                .ok_or(M11BlockSequenceError::CounterOverflow)?,
        prefix_entries: first_entries[..first_local_start].to_vec(),
        suffix_entries: last_entries[last_local_end..].to_vec(),
        boundary_entries_decoded: u64::try_from(first_entries.len())
            .ok()
            .and_then(|count| {
                u64::try_from(last_entries.len())
                    .ok()
                    .and_then(|last| count.checked_add(last))
            })
            .ok_or(M11BlockSequenceError::CounterOverflow)?,
        inspection,
    })
}

fn push_block_storage_page(
    session: &mut ArenaBuildSession<'_>,
    builder: &mut Option<BlockSequenceBuilder>,
    page: &[u8],
    mutation: &mut SequenceMutationReceipt,
) -> Result<(), M11BlockSequenceError> {
    if builder.is_none() {
        *builder = Some(BlockSequenceBuilder::try_new(session, mutation)?);
    }
    let leaf = session.allocate(page, &[])?;
    let builder = builder
        .as_mut()
        .ok_or(M11BlockSequenceError::InvalidState)?;
    builder.begin_push(session, leaf, mutation)?;
    while builder.poll_push(session, mutation)? != ResumableSequenceProgress::Complete {}
    Ok(())
}

fn build_block_splice_replacement(
    session: &mut ArenaBuildSession<'_>,
    prefix: &[M11BlockSequenceEntry],
    replacement: &[M11BlockSequenceEntry],
    suffix: &[M11BlockSequenceEntry],
    mutation: &mut SequenceMutationReceipt,
) -> Result<(Option<BlockSequenceBuildRoot>, usize), M11BlockSequenceError> {
    let mut builder = None;
    let mut page = [0_u8; ARENA_PAGE_BYTES];
    let mut page_len = BLOCK_LEAF_HEADER_BYTES;
    let mut page_entries = 0_u16;
    let mut page_summary = BlockSequenceSummary::empty();
    let mut maximum_entry_bytes = 0_usize;

    for entry in prefix.iter().chain(replacement).chain(suffix) {
        let encoded_len = entry.encoded_len();
        if page_entries > 0
            && (usize::from(page_entries) >= M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX
                || page_len
                    .checked_add(encoded_len)
                    .is_none_or(|next| next > ARENA_PAGE_BYTES))
        {
            encode_leaf_header(
                &mut page,
                page_entries,
                page_len - BLOCK_LEAF_HEADER_BYTES,
                page_summary,
            )?;
            push_block_storage_page(session, &mut builder, &page[..page_len], mutation)?;
            page.fill(0);
            page_len = BLOCK_LEAF_HEADER_BYTES;
            page_entries = 0;
            page_summary = BlockSequenceSummary::empty();
        }
        if page_len
            .checked_add(encoded_len)
            .is_none_or(|next| next > ARENA_PAGE_BYTES)
        {
            return Err(M11BlockSequenceError::RoleRecordTooLarge {
                bytes: encoded_len,
                cap: ARENA_PAGE_BYTES - BLOCK_LEAF_HEADER_BYTES,
            });
        }
        page_summary = page_summary.checked_followed_by(entry_summary(entry))?;
        encode_entry(entry, &mut page, &mut page_len)?;
        page_entries = page_entries
            .checked_add(1)
            .ok_or(M11BlockSequenceError::CounterOverflow)?;
        maximum_entry_bytes = maximum_entry_bytes.max(encoded_len);
    }

    if page_entries > 0 {
        encode_leaf_header(
            &mut page,
            page_entries,
            page_len - BLOCK_LEAF_HEADER_BYTES,
            page_summary,
        )?;
        push_block_storage_page(session, &mut builder, &page[..page_len], mutation)?;
    }
    let Some(mut builder) = builder else {
        return Ok((None, maximum_entry_bytes));
    };
    builder.begin_finish(session, mutation)?;
    while builder.poll_finish(session, mutation)? != ResumableSequenceProgress::Complete {}
    Ok((Some(builder.take_root(session)?), maximum_entry_bytes))
}

fn abort_block_sequence_seal_after_failure(runtime: &mut DocumentRuntime, seal: BlockSequenceSeal) {
    if let Err(failure) = seal.abort(runtime.producer_arena_mut()) {
        let error = failure.error;
        let _seal = failure.seal;
        panic!("block splice seal rejected its own arena during cleanup: {error}");
    }
}

fn release_block_sequence_tree_after_failure(
    runtime: &mut DocumentRuntime,
    tree: BlockSequenceTree,
) {
    if let Err(failure) = tree.release(runtime.producer_arena_mut()) {
        panic!(
            "block splice tree was rejected by the arena that created it: {}",
            failure.error
        );
    }
}

/// Path-copies a semantic block-leaf replacement against one durable base.
///
/// The base may name the previous source revision. Its live source lease and
/// runtime identity prove a durable producer root, but this layer cannot prove
/// that a separate host acknowledged it; the endpoint must select its
/// acknowledged root before calling. `target_lease` must name the runtime's
/// exact current source. Boundary-page decoding is capped by
/// `2 * M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX`; replacement assembly is
/// proportional only to the supplied replacement plus those boundary entries,
/// and measured-tree mutation is bounded by AVL height.
///
/// This engine-level vertical slice is intentionally synchronous. A parser
/// integration should drive replacement production and sealing off the UI
/// thread or wrap the same phases in its existing fuelled job.
enum BlockSequenceSpliceBase<'base> {
    Committed(Option<&'base BlockSequenceTree>),
    Persistent(Option<ArenaId>),
}

impl BlockSequenceSpliceBase<'_> {
    fn as_ref(&self) -> MeasuredSequenceRef<'_, BlockSequenceSpec> {
        match self {
            Self::Committed(Some(tree)) => tree.as_ref(),
            Self::Committed(None) | Self::Persistent(None) => {
                MeasuredSequenceRef::from_imported_root(None)
            }
            Self::Persistent(root) => MeasuredSequenceRef::from_imported_root(*root),
        }
    }
}

#[doc(hidden)]
pub fn splice_m11_block_sequence_atomic(
    runtime: &mut DocumentRuntime,
    base: &M11BlockSequenceRoot,
    target_lease: SourceSnapshotLease,
    entry_range: Range<u64>,
    replacement: &[M11BlockSequenceEntry],
) -> Result<(M11BlockSequenceRoot, M11BlockSequenceSpliceReceipt), M11BlockSequenceError> {
    if runtime.producer_identity() != base.runtime_identity {
        return Err(M11BlockSequenceError::WrongRuntime);
    }
    if base.released || base.lease.is_none() {
        return Err(M11BlockSequenceError::InvalidState);
    }
    splice_m11_block_sequence_from_base_atomic(
        runtime,
        base.runtime_identity,
        base.summary,
        base.page_count,
        BlockSequenceSpliceBase::Committed(base.tree.as_ref()),
        target_lease,
        entry_range,
        replacement,
    )
}

/// Path-copies one block replacement from the exact retained publication.
///
/// `base_claim` must come from the paired Green/Projection descriptors of the
/// same retained manifest that owns `base_root`. The semantic selection is
/// rechecked against both the base and the locally produced target.
pub(crate) fn splice_persistent_m11_block_sequence_atomic(
    runtime: &mut DocumentRuntime,
    base_root: Option<ArenaId>,
    base_claim: PersistentM11BlockRootClaim,
    target_lease: SourceSnapshotLease,
    selection: &M11BlockSequenceSpliceSelection,
    replacement: &[M11BlockSequenceEntry],
) -> Result<(M11BlockSequenceRoot, M11BlockSequenceSpliceReceipt), M11BlockSequenceError> {
    let base_entry_range = selection.base_entry_range();
    let target_entry_range = selection.target_entry_range();
    let replacement_entries =
        u64::try_from(replacement.len()).map_err(|_| M11BlockSequenceError::CounterOverflow)?;
    if target_entry_range.end - target_entry_range.start != replacement_entries
        || base_entry_range.end > base_claim.entry_count()
    {
        return Err(M11BlockSequenceError::InvalidPoint);
    }
    let target_entries = base_claim
        .entry_count()
        .checked_sub(base_entry_range.end - base_entry_range.start)
        .and_then(|entries| entries.checked_add(replacement_entries))
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    if target_entry_range.end > target_entries {
        return Err(M11BlockSequenceError::InvalidPoint);
    }
    let runtime_identity = runtime.producer_identity();
    splice_m11_block_sequence_from_base_atomic(
        runtime,
        runtime_identity,
        base_claim.summary,
        base_claim.storage_page_count,
        BlockSequenceSpliceBase::Persistent(base_root),
        target_lease,
        base_entry_range,
        replacement,
    )
}

#[allow(clippy::too_many_arguments)]
fn splice_m11_block_sequence_from_base_atomic(
    runtime: &mut DocumentRuntime,
    runtime_identity: RuntimeIdentity,
    base_summary: BlockSequenceSummary,
    base_page_count: u64,
    base_tree: BlockSequenceSpliceBase<'_>,
    target_lease: SourceSnapshotLease,
    entry_range: Range<u64>,
    replacement: &[M11BlockSequenceEntry],
) -> Result<(M11BlockSequenceRoot, M11BlockSequenceSpliceReceipt), M11BlockSequenceError> {
    let target_source = target_lease.version();
    if runtime.current_source_version() != Some(target_source) {
        return Err(M11BlockSequenceError::SourceAuthorityMismatch);
    }
    let target_byte_len = u64::try_from(target_source.byte_len())
        .map_err(|_| M11BlockSequenceError::CounterOverflow)?;
    let target_utf16_len = u64::try_from(target_source.utf16_len())
        .map_err(|_| M11BlockSequenceError::CounterOverflow)?;
    if entry_range.start > entry_range.end || entry_range.end > base_summary.entries {
        return Err(M11BlockSequenceError::InvalidPoint);
    }
    let deleted_entries = entry_range.end - entry_range.start;
    let replacement_entries =
        u64::try_from(replacement.len()).map_err(|_| M11BlockSequenceError::CounterOverflow)?;
    let target_entries = base_summary
        .entries
        .checked_sub(deleted_entries)
        .and_then(|entries| entries.checked_add(replacement_entries))
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    let plan = plan_block_semantic_splice(
        runtime.producer_arena(),
        base_tree.as_ref(),
        base_summary,
        base_page_count,
        entry_range,
    )?;
    let deleted_storage_pages = plan
        .storage_page_range
        .end
        .checked_sub(plan.storage_page_range.start)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    let boundary_entries_reencoded = u64::try_from(
        plan.prefix_entries
            .len()
            .checked_add(plan.suffix_entries.len())
            .ok_or(M11BlockSequenceError::CounterOverflow)?,
    )
    .map_err(|_| M11BlockSequenceError::CounterOverflow)?;

    let mut mutation = SequenceMutationReceipt::default();
    add_inspection(&mut mutation.inspection, plan.inspection)?;
    let mut session = runtime.producer_arena_mut().begin_build()?;
    let (replacement_root, maximum_entry_bytes) = build_block_splice_replacement(
        &mut session,
        &plan.prefix_entries,
        replacement,
        &plan.suffix_entries,
        &mut mutation,
    )?;
    let replacement_storage_pages = u64::try_from(mutation.leaves_adopted)
        .map_err(|_| M11BlockSequenceError::CounterOverflow)?;
    let receipt_context = BlockSequenceSpliceReceiptContext {
        base_entries: base_summary.entries,
        deleted_entries,
        replacement_entries,
        boundary_entries_decoded: plan.boundary_entries_decoded,
        boundary_entries_reencoded,
        base_storage_pages: base_page_count,
        deleted_storage_pages,
        replacement_storage_pages,
    };
    let root = match base_tree {
        BlockSequenceSpliceBase::Committed(Some(base_tree)) => {
            splice_measured_sequence_atomic::<BlockSequenceSpec>(
                &mut session,
                base_tree,
                plan.storage_page_range,
                replacement_root,
                &mut mutation,
            )?
        }
        BlockSequenceSpliceBase::Persistent(Some(base_root)) => {
            let owner = session.retain(base_root)?;
            mutation.committed_leaves_retained = mutation
                .committed_leaves_retained
                .checked_add(
                    usize::try_from(base_page_count)
                        .map_err(|_| M11BlockSequenceError::CounterOverflow)?,
                )
                .ok_or(M11BlockSequenceError::CounterOverflow)?;
            let base_root = validate_measured_sequence_build_owner::<BlockSequenceSpec>(
                &session,
                owner,
                &mut mutation,
            )?;
            splice_measured_sequence_build_root_atomic::<BlockSequenceSpec>(
                &mut session,
                base_root,
                plan.storage_page_range,
                replacement_root,
                &mut mutation,
            )?
        }
        BlockSequenceSpliceBase::Committed(None) | BlockSequenceSpliceBase::Persistent(None) => {
            replacement_root
        }
    };

    let Some(root) = root else {
        drop(session);
        if target_entries != 0 || target_byte_len != 0 || target_utf16_len != 0 {
            return Err(M11BlockSequenceError::IncompleteCoverage);
        }
        let summary = BlockSequenceSummary::empty();
        let build_receipt = M11BlockSequenceBuildReceipt::from_mutation(
            0,
            summary,
            maximum_entry_bytes,
            mutation,
            0,
        );
        let splice_receipt =
            M11BlockSequenceSpliceReceipt::from_mutation(receipt_context, mutation, 0)?;
        return Ok((
            M11BlockSequenceRoot::empty(runtime_identity, target_lease, build_receipt),
            splice_receipt,
        ));
    };

    let build = session.suspend()?;
    let mut seal = match begin_measured_sequence_seal(runtime.producer_arena_mut(), build, root) {
        Ok(seal) => seal,
        Err(failure) => {
            let error = failure.error;
            let _root = failure.root;
            if let Err(abort_error) = runtime.producer_arena_mut().abort_build(failure.build) {
                panic!("block splice build rejected its own arena during cleanup: {abort_error}");
            }
            return Err(error.into());
        }
    };
    let mut seal_transitions = 0_usize;
    let tree = loop {
        let poll = match seal.poll(runtime.producer_arena_mut(), 1) {
            Ok(poll) => poll,
            Err(error) => {
                abort_block_sequence_seal_after_failure(runtime, seal);
                return Err(error.into());
            }
        };
        let Some(next_seal_transitions) = seal_transitions.checked_add(poll.transitions) else {
            abort_block_sequence_seal_after_failure(runtime, seal);
            return Err(M11BlockSequenceError::CounterOverflow);
        };
        seal_transitions = next_seal_transitions;
        if let Some(tree) = poll.root {
            break tree;
        }
    };

    let mut inspection = SequenceInspectionReceipt::default();
    let measure = match tree
        .as_ref()
        .summary(runtime.producer_arena(), &mut inspection)
    {
        Ok(Some(measure)) => measure,
        Ok(None) => {
            release_block_sequence_tree_after_failure(runtime, tree);
            return Err(M11BlockSequenceError::Corrupt(
                "nonempty block splice sealed an empty root",
            ));
        }
        Err(error) => {
            release_block_sequence_tree_after_failure(runtime, tree);
            return Err(error);
        }
    };
    if let Err(error) = add_inspection(&mut mutation.inspection, inspection) {
        release_block_sequence_tree_after_failure(runtime, tree);
        return Err(error);
    }
    let summary = measure.summary();
    if summary.entries != target_entries
        || summary.source_bytes != target_byte_len
        || summary.source_utf16 != target_utf16_len
    {
        release_block_sequence_tree_after_failure(runtime, tree);
        return Err(M11BlockSequenceError::IncompleteCoverage);
    }
    let build_receipt = M11BlockSequenceBuildReceipt::from_mutation(
        0,
        summary,
        maximum_entry_bytes,
        mutation,
        seal_transitions,
    );
    let splice_receipt = match M11BlockSequenceSpliceReceipt::from_mutation(
        receipt_context,
        mutation,
        seal_transitions,
    ) {
        Ok(receipt) => receipt,
        Err(error) => {
            release_block_sequence_tree_after_failure(runtime, tree);
            return Err(error);
        }
    };
    Ok((
        M11BlockSequenceRoot {
            runtime_identity,
            lease: Some(target_lease),
            source: target_source,
            summary,
            page_count: measure.leaves(),
            tree_height: measure.height(),
            tree: Some(tree),
            receipt: build_receipt,
            released: false,
        },
        splice_receipt,
    ))
}

impl Drop for M11BlockSequenceRoot {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.released,
                "block sequence roots require explicit release"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11BlockSequenceReclaimPoll {
    receipt: ReclaimReceipt,
    complete: bool,
}

impl M11BlockSequenceReclaimPoll {
    #[must_use]
    pub const fn receipt(self) -> ReclaimReceipt {
        self.receipt
    }

    #[must_use]
    pub const fn complete(self) -> bool {
        self.complete
    }
}

fn poll_reclaim(
    runtime: &mut DocumentRuntime,
    fuel: usize,
) -> Result<M11BlockSequenceReclaimPoll, M11BlockSequenceError> {
    validate_fuel(fuel)?;
    let receipt = runtime.producer_arena_mut().poll_reclaim(fuel);
    let metrics = runtime.arena_metrics();
    Ok(M11BlockSequenceReclaimPoll {
        receipt,
        complete: metrics.pending_build_aborts == 0 && metrics.pending_reclaims == 0,
    })
}

fn validate_fuel(fuel: usize) -> Result<(), M11BlockSequenceError> {
    if fuel == 0 {
        return Err(M11BlockSequenceError::ZeroFuel);
    }
    if fuel > M11_BLOCK_SEQUENCE_MAX_POLL_TRANSITIONS {
        return Err(M11BlockSequenceError::PollLimitExceeded);
    }
    Ok(())
}

fn add_inspection(
    total: &mut SequenceInspectionReceipt,
    added: SequenceInspectionReceipt,
) -> Result<(), M11BlockSequenceError> {
    total.node_headers_decoded = total
        .node_headers_decoded
        .checked_add(added.node_headers_decoded)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    total.summary_combinations = total
        .summary_combinations
        .checked_add(added.summary_combinations)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    total.spec.payload_bytes_inspected = total
        .spec
        .payload_bytes_inspected
        .checked_add(added.spec.payload_bytes_inspected)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    total.spec.spec_items_hashed = total
        .spec
        .spec_items_hashed
        .checked_add(added.spec.spec_items_hashed)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    Ok(())
}

fn write_bytes(
    output: &mut [u8],
    cursor: &mut usize,
    bytes: &[u8],
) -> Result<(), M11BlockSequenceError> {
    let end = cursor
        .checked_add(bytes.len())
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    let destination = output
        .get_mut(*cursor..end)
        .ok_or(M11BlockSequenceError::Corrupt(
            "block encoding exceeds its page",
        ))?;
    destination.copy_from_slice(bytes);
    *cursor = end;
    Ok(())
}

fn write_u8(output: &mut [u8], cursor: &mut usize, value: u8) -> Result<(), M11BlockSequenceError> {
    write_bytes(output, cursor, &[value])
}

fn write_u16(
    output: &mut [u8],
    cursor: &mut usize,
    value: u16,
) -> Result<(), M11BlockSequenceError> {
    write_bytes(output, cursor, &value.to_le_bytes())
}

fn write_u32(
    output: &mut [u8],
    cursor: &mut usize,
    value: u32,
) -> Result<(), M11BlockSequenceError> {
    write_bytes(output, cursor, &value.to_le_bytes())
}

fn write_u64(
    output: &mut [u8],
    cursor: &mut usize,
    value: u64,
) -> Result<(), M11BlockSequenceError> {
    write_bytes(output, cursor, &value.to_le_bytes())
}

fn read_bytes<'a>(
    input: &'a [u8],
    cursor: &mut usize,
    len: usize,
) -> Result<&'a [u8], M11BlockSequenceError> {
    let end = cursor
        .checked_add(len)
        .ok_or(M11BlockSequenceError::CounterOverflow)?;
    let bytes = input
        .get(*cursor..end)
        .ok_or(M11BlockSequenceError::Corrupt(
            "block payload field is truncated",
        ))?;
    *cursor = end;
    Ok(bytes)
}

fn read_u8(input: &[u8], cursor: &mut usize) -> Result<u8, M11BlockSequenceError> {
    Ok(*read_bytes(input, cursor, 1)?
        .first()
        .expect("checked byte width"))
}

fn read_u16(input: &[u8], cursor: &mut usize) -> Result<u16, M11BlockSequenceError> {
    Ok(u16::from_le_bytes(
        read_bytes(input, cursor, 2)?
            .try_into()
            .expect("checked u16 width"),
    ))
}

fn read_u32(input: &[u8], cursor: &mut usize) -> Result<u32, M11BlockSequenceError> {
    Ok(u32::from_le_bytes(
        read_bytes(input, cursor, 4)?
            .try_into()
            .expect("checked u32 width"),
    ))
}

fn read_u64(input: &[u8], cursor: &mut usize) -> Result<u64, M11BlockSequenceError> {
    Ok(u64::from_le_bytes(
        read_bytes(input, cursor, 8)?
            .try_into()
            .expect("checked u64 width"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentRuntimeConfig;

    fn role(bytes: &[u8]) -> M11BlockRoleRecord {
        M11BlockRoleRecord::new(bytes).expect("role record")
    }

    fn drive_entry(
        build: &mut M11BlockSequenceBuild,
        runtime: &mut DocumentRuntime,
        entry: M11BlockSequenceEntry,
    ) {
        build.offer_entry(entry).expect("offer entry");
        loop {
            let poll = build.poll(runtime, 16).expect("entry poll");
            if poll.status() == M11BlockSequenceBuildStatus::NeedsInput {
                break;
            }
            assert_eq!(poll.status(), M11BlockSequenceBuildStatus::Pending);
        }
    }

    fn finish_build(
        mut build: M11BlockSequenceBuild,
        runtime: &mut DocumentRuntime,
    ) -> M11BlockSequenceRoot {
        build.finish_input().expect("finish input");
        loop {
            if build.poll(runtime, 64).expect("finish poll").status()
                == M11BlockSequenceBuildStatus::Complete
            {
                break;
            }
        }
        let root = build.take_root().expect("block root");
        drop(build);
        root
    }

    fn release_root(root: &mut M11BlockSequenceRoot, runtime: &mut DocumentRuntime) {
        root.begin_release(runtime).expect("begin root release");
        while !root
            .poll_release(runtime, 64)
            .expect("poll root release")
            .complete()
        {}
    }

    fn close_runtime(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin close");
        while !runtime.poll_close(64).expect("poll close").complete {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
        assert_eq!(runtime.arena_metrics().live_builds, 0);
    }

    fn reachable_block_tree_ids(
        arena: &PageArena,
        root: ArenaId,
    ) -> (
        std::collections::HashSet<ArenaId>,
        std::collections::HashSet<ArenaId>,
    ) {
        let mut leaves = std::collections::HashSet::new();
        let mut branches = std::collections::HashSet::new();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            let payload = arena.payload(id).expect("reachable block payload");
            if payload.get(..4) == Some(BLOCK_LEAF_MAGIC.as_slice()) {
                assert_eq!(arena.child_count(id).expect("leaf child count"), 0);
                leaves.insert(id);
            } else {
                assert_eq!(payload.get(..4), Some(BLOCK_BRANCH_MAGIC.as_slice()));
                let child_count = arena.child_count(id).expect("branch child count");
                assert_eq!(child_count, 2);
                branches.insert(id);
                for index in 0..child_count {
                    pending.push(arena.child_at(id, index).expect("branch child"));
                }
            }
        }
        (leaves, branches)
    }

    #[test]
    fn structured_entry_rejects_missing_records_and_unsupported_reason() {
        assert!(matches!(
            M11BlockSequenceEntry::new(
                M11BlockSequenceEntryKind::Structured,
                1,
                1,
                0,
                None,
                Some(role(b"projection")),
                None,
            ),
            Err(M11BlockSequenceError::InvalidEntryShape)
        ));
        assert!(matches!(
            M11BlockSequenceEntry::new(
                M11BlockSequenceEntryKind::Structured,
                1,
                1,
                0,
                Some(role(b"green")),
                Some(role(b"projection")),
                Some(M11BlockUnsupportedReason::new(1).expect("reason")),
            ),
            Err(M11BlockSequenceError::InvalidEntryShape)
        ));

        let mut inconsistent = entry_summary(
            &M11BlockSequenceEntry::structured(1, 1, 0, role(b"green"), role(b"projection"))
                .expect("structured"),
        );
        inconsistent.structured = 0;
        assert!(matches!(
            validate_summary_counts(inconsistent),
            Err(M11BlockSequenceError::Corrupt(
                "block summary counters are inconsistent"
            ))
        ));
    }

    #[test]
    fn structured_entry_roundtrips_queries_and_splices_with_summary() {
        let mut runtime =
            DocumentRuntime::new("abc", DocumentRuntimeConfig::default()).expect("runtime");
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build = M11BlockSequenceBuild::new(&runtime, lease).expect("build");
        drive_entry(
            &mut build,
            &mut runtime,
            M11BlockSequenceEntry::paragraph(1, 1, 0, role(b"g-a"), role(b"p-a"))
                .expect("paragraph"),
        );
        drive_entry(
            &mut build,
            &mut runtime,
            M11BlockSequenceEntry::structured(1, 1, 2, role(b"g-b"), role(b"p-b"))
                .expect("structured"),
        );
        drive_entry(
            &mut build,
            &mut runtime,
            M11BlockSequenceEntry::paragraph(1, 1, 0, role(b"g-c"), role(b"p-c"))
                .expect("paragraph"),
        );
        let mut base = finish_build(build, &mut runtime);
        assert_eq!(base.entry_count(), 3);
        assert_eq!(base.paragraph_count(), 2);
        assert_eq!(base.structured_count(), 1);
        assert_eq!(base.reference_definition_count(), 2);

        let located = base
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(1, 1, SourceBoundaryAffinity::After),
            )
            .expect("structured query")
            .expect("structured entry");
        assert_eq!(located.entry_ordinal(), 1);
        assert_eq!(
            located.entry().kind(),
            M11BlockSequenceEntryKind::Structured
        );
        assert_eq!(located.entry().green().expect("Green").as_bytes(), b"g-b");
        assert_eq!(
            located.entry().projection().expect("Projection").as_bytes(),
            b"p-b"
        );
        assert_eq!(located.entry().reference_definition_count(), 2);
        assert_eq!(located.entry().unsupported_reason(), None);

        for lane in [M11BlockRoleLane::Green, M11BlockRoleLane::Projection] {
            let descriptor = descriptor_for(
                base.summary,
                base.storage_page_count(),
                base.tree_height(),
                lane,
            );
            let encoded =
                encode_persistent_m11_block_role_descriptor(descriptor).expect("encode descriptor");
            assert_eq!(
                decode_persistent_m11_block_role_descriptor(&encoded, lane, 3, 3)
                    .expect("decode descriptor"),
                descriptor
            );
        }

        runtime
            .apply_edit(base.source(), 1..2, "d")
            .expect("same-width structured edit");
        let target_lease = runtime.snapshot_current_source().expect("target source");
        let replacement = [
            M11BlockSequenceEntry::structured(1, 1, 3, role(b"g-d"), role(b"p-d"))
                .expect("replacement structured"),
        ];
        let (mut target, receipt) =
            splice_m11_block_sequence_atomic(&mut runtime, &base, target_lease, 1..2, &replacement)
                .expect("structured splice");
        assert_eq!(receipt.deleted_entries(), 1);
        assert_eq!(receipt.replacement_entries(), 1);
        assert_eq!(target.entry_count(), 3);
        assert_eq!(target.paragraph_count(), 2);
        assert_eq!(target.structured_count(), 1);
        assert_eq!(target.reference_definition_count(), 3);

        let replaced = target
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(1, 1, SourceBoundaryAffinity::After),
            )
            .expect("replacement query")
            .expect("replacement structured");
        assert_eq!(
            replaced.entry().kind(),
            M11BlockSequenceEntryKind::Structured
        );
        assert_eq!(replaced.entry().green().expect("Green").as_bytes(), b"g-d");
        assert_eq!(
            replaced
                .entry()
                .projection()
                .expect("Projection")
                .as_bytes(),
            b"p-d"
        );

        release_root(&mut target, &mut runtime);
        release_root(&mut base, &mut runtime);
        drop(target);
        drop(base);
        close_runtime(runtime);
    }

    #[test]
    fn more_than_128_entries_pack_into_few_pages_and_route_logarithmically() {
        let pairs = 300;
        let text = "x\n".repeat(pairs);
        let mut runtime =
            DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build = M11BlockSequenceBuild::new(&runtime, lease).expect("build");
        for _ in 0..pairs {
            drive_entry(
                &mut build,
                &mut runtime,
                M11BlockSequenceEntry::paragraph(1, 1, 0, role(b"g"), role(b"p"))
                    .expect("paragraph"),
            );
            drive_entry(
                &mut build,
                &mut runtime,
                M11BlockSequenceEntry::blank(1, 1).expect("blank"),
            );
        }
        let mut root = finish_build(build, &mut runtime);
        assert_eq!(root.entry_count(), 600);
        assert_eq!(root.paragraph_count(), 300);
        assert_eq!(root.blank_count(), 300);
        assert!(
            root.storage_page_count() <= 10,
            "64-entry packing should need at most ten pages, got {}",
            root.storage_page_count()
        );
        assert!(root.storage_page_count() * 16 < root.entry_count());
        assert!(root.tree_height() > 1);
        assert_ne!(root.green_commitment256(), root.projection_commitment256());

        let first = root
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
            )
            .expect("first query")
            .expect("first entry");
        assert_eq!(first.entry().kind(), M11BlockSequenceEntryKind::Paragraph);
        assert_eq!(first.byte_range(), 0..1);

        let before_boundary = root
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(1, 1, SourceBoundaryAffinity::Before),
            )
            .expect("boundary query")
            .expect("boundary entry");
        assert_eq!(
            before_boundary.entry().kind(),
            M11BlockSequenceEntryKind::Paragraph
        );
        assert_eq!(before_boundary.byte_range(), 0..1);

        let after_boundary = root
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(1, 1, SourceBoundaryAffinity::After),
            )
            .expect("boundary query")
            .expect("boundary entry");
        assert_eq!(
            after_boundary.entry().kind(),
            M11BlockSequenceEntryKind::Blank
        );
        assert_eq!(after_boundary.byte_range(), 1..2);

        let eof_before = root
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(text.len(), text.len(), SourceBoundaryAffinity::Before),
            )
            .expect("EOF query")
            .expect("EOF entry");
        let eof_after = root
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(text.len(), text.len(), SourceBoundaryAffinity::After),
            )
            .expect("EOF query")
            .expect("EOF entry");
        assert_eq!(eof_before.entry_ordinal(), 599);
        assert_eq!(eof_after.entry_ordinal(), 599);
        assert_eq!(eof_after.byte_range(), 599..600);
        assert!(
            usize::from(eof_after.receipt().entries_scanned())
                <= M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX
        );
        assert!(
            eof_after.receipt().node_headers_decoded() < root.entry_count(),
            "tree routing must not inspect all semantic entries"
        );

        release_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }

    #[test]
    fn point_query_header_bound_covers_packed_page_boundaries() {
        for entry_count in [1_usize, 64, 65, 128, 129, 260] {
            let text = "x".repeat(entry_count);
            let mut runtime =
                DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
            let lease = runtime.snapshot_current_source().expect("source");
            let mut build = M11BlockSequenceBuild::new(&runtime, lease).expect("build");
            for _ in 0..entry_count {
                drive_entry(
                    &mut build,
                    &mut runtime,
                    M11BlockSequenceEntry::paragraph(1, 1, 0, role(b"g"), role(b"p"))
                        .expect("paragraph"),
                );
            }
            let mut root = finish_build(build, &mut runtime);
            assert_eq!(
                root.storage_page_count(),
                u64::try_from(entry_count.div_ceil(M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX))
                    .expect("test page count")
            );
            let maximum_headers = maximum_metric_lookup_node_headers(root.tree_height());
            let descriptor_claim = PersistentM11BlockRootClaim {
                summary: root.summary,
                storage_page_count: root.page_count,
                tree_height: root.tree_height,
            };
            assert_eq!(
                descriptor_claim.maximum_point_query_node_headers(),
                maximum_headers
            );
            let points = (0..=entry_count).flat_map(|offset| {
                [
                    (offset, SourceBoundaryAffinity::Before),
                    (offset, SourceBoundaryAffinity::After),
                ]
            });
            let mut maximum_observed_headers = 0;
            for (offset, affinity) in points {
                let located = root
                    .locate_point(
                        &runtime,
                        M11BlockSequencePoint::new(offset, offset, affinity),
                    )
                    .expect("bounded point query")
                    .expect("covered point");
                maximum_observed_headers =
                    maximum_observed_headers.max(located.receipt().node_headers_decoded());
                assert!(
                    located.receipt().node_headers_decoded()
                        <= descriptor_claim.maximum_point_query_node_headers(),
                    "entry_count={entry_count}, offset={offset}, affinity={affinity:?}"
                );
            }
            assert_eq!(
                maximum_observed_headers, maximum_headers,
                "the height-derived descriptor bound must be tight for entry_count={entry_count}"
            );

            release_root(&mut root, &mut runtime);
            drop(root);
            close_runtime(runtime);
        }
    }

    #[test]
    fn consecutive_visit_seeks_inside_a_page_and_walks_pages_linearly() {
        const ENTRY_COUNT: usize = 600;
        let text = "x".repeat(ENTRY_COUNT);
        let mut runtime =
            DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build = M11BlockSequenceBuild::new(&runtime, lease).expect("build");
        for _ in 0..ENTRY_COUNT {
            drive_entry(
                &mut build,
                &mut runtime,
                M11BlockSequenceEntry::paragraph(1, 1, 0, role(b"g"), role(b"p"))
                    .expect("paragraph"),
            );
        }
        let mut root = finish_build(build, &mut runtime);
        let claim = PersistentM11BlockRootClaim {
            summary: root.summary,
            storage_page_count: root.page_count,
            tree_height: root.tree_height,
        };
        let mut observed = Vec::new();
        let receipt = persistent_m11_block_visit_entries(
            runtime.producer_arena(),
            root.tree_root_id_for_test(),
            claim,
            M11BlockSequenceVisitStart {
                entry_ordinal: 60,
                byte_offset: 60,
                utf16_offset: 60,
            },
            500,
            16,
            |entry| {
                observed.push((
                    entry.entry_ordinal(),
                    entry.byte_start()..entry.byte_end(),
                    entry.utf16_start()..entry.utf16_end(),
                ));
                M11BlockSequenceVisitControl::Continue
            },
        )
        .expect("consecutive visit");
        assert_eq!(observed.len(), 500);
        assert_eq!(observed.first(), Some(&(60, 60..61, 60..61)));
        assert_eq!(observed.last(), Some(&(559, 559..560, 559..560)));
        assert_eq!(receipt.visited_entries(), 500);
        assert_eq!(receipt.storage_pages_visited(), 9);
        assert_eq!(receipt.next_entry_ordinal(), 560);
        assert_eq!(receipt.next_byte_offset(), 560);
        assert_eq!(receipt.next_utf16_offset(), 560);
        assert_eq!(
            receipt.disposition(),
            M11BlockSequenceVisitDisposition::EntryLimit
        );
        let inspection = receipt.inspection();
        let conservative_header_bound = maximum_consecutive_block_visit_node_headers(
            root.tree_height(),
            u32::try_from(receipt.storage_pages_visited()).expect("test page count"),
        );
        assert!(
            inspection.node_headers_decoded <= conservative_header_bound,
            "actual={} bound={conservative_header_bound}",
            inspection.node_headers_decoded
        );
        assert!(
            inspection.node_headers_decoded < receipt.visited_entries(),
            "contiguous work must scale with pages rather than semantic entries"
        );

        release_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }

    #[test]
    fn consecutive_visit_stops_before_the_next_packed_page_and_on_visitor_request() {
        const ENTRY_COUNT: usize = 130;
        let text = "x".repeat(ENTRY_COUNT);
        let mut runtime =
            DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build = M11BlockSequenceBuild::new(&runtime, lease).expect("build");
        for _ in 0..ENTRY_COUNT {
            drive_entry(
                &mut build,
                &mut runtime,
                M11BlockSequenceEntry::paragraph(1, 1, 0, role(b"g"), role(b"p"))
                    .expect("paragraph"),
            );
        }
        let mut root = finish_build(build, &mut runtime);
        let claim = PersistentM11BlockRootClaim {
            summary: root.summary,
            storage_page_count: root.page_count,
            tree_height: root.tree_height,
        };

        let mut page_limited_ordinals = Vec::new();
        let page_limited = persistent_m11_block_visit_entries(
            runtime.producer_arena(),
            root.tree_root_id_for_test(),
            claim,
            M11BlockSequenceVisitStart {
                entry_ordinal: 63,
                byte_offset: 63,
                utf16_offset: 63,
            },
            100,
            1,
            |entry| {
                page_limited_ordinals.push(entry.entry_ordinal());
                M11BlockSequenceVisitControl::Continue
            },
        )
        .expect("page-limited visit");
        assert_eq!(page_limited_ordinals, [63]);
        assert_eq!(page_limited.storage_pages_visited(), 1);
        assert_eq!(page_limited.next_entry_ordinal(), 64);
        assert_eq!(page_limited.next_byte_offset(), 64);
        assert_eq!(
            page_limited.disposition(),
            M11BlockSequenceVisitDisposition::StoragePageLimit
        );

        let mut visitor_ordinals = Vec::new();
        let visitor_stopped = persistent_m11_block_visit_entries(
            runtime.producer_arena(),
            root.tree_root_id_for_test(),
            claim,
            M11BlockSequenceVisitStart {
                entry_ordinal: 10,
                byte_offset: 10,
                utf16_offset: 10,
            },
            100,
            3,
            |entry| {
                visitor_ordinals.push(entry.entry_ordinal());
                if visitor_ordinals.len() == 5 {
                    M11BlockSequenceVisitControl::Stop
                } else {
                    M11BlockSequenceVisitControl::Continue
                }
            },
        )
        .expect("visitor-stopped visit");
        assert_eq!(visitor_ordinals, [10, 11, 12, 13, 14]);
        assert_eq!(visitor_stopped.next_entry_ordinal(), 15);
        assert_eq!(visitor_stopped.next_byte_offset(), 15);
        assert_eq!(
            visitor_stopped.disposition(),
            M11BlockSequenceVisitDisposition::VisitorStopped
        );

        assert!(matches!(
            persistent_m11_block_visit_entries(
                runtime.producer_arena(),
                root.tree_root_id_for_test(),
                claim,
                M11BlockSequenceVisitStart {
                    entry_ordinal: 10,
                    byte_offset: 11,
                    utf16_offset: 10,
                },
                1,
                1,
                |_| M11BlockSequenceVisitControl::Continue,
            ),
            Err(M11BlockSequenceError::InvalidPoint)
        ));

        release_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }

    #[test]
    fn consecutive_visit_preserves_unicode_dual_coordinate_ranges() {
        let text = "éa😊b";
        let mut runtime =
            DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build = M11BlockSequenceBuild::new(&runtime, lease).expect("build");
        for (bytes, utf16) in [(2, 1), (1, 1), (4, 2), (1, 1)] {
            drive_entry(
                &mut build,
                &mut runtime,
                M11BlockSequenceEntry::paragraph(bytes, utf16, 0, role(b"g"), role(b"p"))
                    .expect("paragraph"),
            );
        }
        let mut root = finish_build(build, &mut runtime);
        let claim = PersistentM11BlockRootClaim {
            summary: root.summary,
            storage_page_count: root.page_count,
            tree_height: root.tree_height,
        };
        let mut observed = Vec::new();
        let receipt = persistent_m11_block_visit_entries(
            runtime.producer_arena(),
            root.tree_root_id_for_test(),
            claim,
            M11BlockSequenceVisitStart {
                entry_ordinal: 1,
                byte_offset: 2,
                utf16_offset: 1,
            },
            2,
            1,
            |entry| {
                observed.push((
                    entry.byte_start()..entry.byte_end(),
                    entry.utf16_start()..entry.utf16_end(),
                ));
                M11BlockSequenceVisitControl::Continue
            },
        )
        .expect("Unicode visit");
        assert_eq!(observed, [(2..3, 1..2), (3..7, 2..4)]);
        assert_eq!(receipt.next_entry_ordinal(), 3);
        assert_eq!(receipt.next_byte_offset(), 7);
        assert_eq!(receipt.next_utf16_offset(), 4);
        assert_eq!(
            receipt.disposition(),
            M11BlockSequenceVisitDisposition::EntryLimit
        );

        release_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }

    #[test]
    fn dual_coordinate_ranges_and_all_gap_kinds_preserve_affinity() {
        let paragraph = "é";
        let blank = "\n";
        let definitions = "[d]: u\n";
        let unsupported = "#";
        let text = format!("{paragraph}{blank}{definitions}{unsupported}");
        let mut runtime =
            DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build = M11BlockSequenceBuild::new(&runtime, lease).expect("build");
        drive_entry(
            &mut build,
            &mut runtime,
            M11BlockSequenceEntry::paragraph(
                paragraph.len(),
                paragraph.encode_utf16().count(),
                0,
                role(b"green"),
                role(b"projection"),
            )
            .expect("paragraph"),
        );
        drive_entry(
            &mut build,
            &mut runtime,
            M11BlockSequenceEntry::blank(blank.len(), blank.encode_utf16().count()).expect("blank"),
        );
        drive_entry(
            &mut build,
            &mut runtime,
            M11BlockSequenceEntry::definitions_only(
                definitions.len(),
                definitions.encode_utf16().count(),
                1,
            )
            .expect("definitions"),
        );
        drive_entry(
            &mut build,
            &mut runtime,
            M11BlockSequenceEntry::unsupported(
                unsupported.len(),
                unsupported.encode_utf16().count(),
                M11BlockUnsupportedReason::new(7).expect("reason"),
            )
            .expect("unsupported"),
        );
        let mut root = finish_build(build, &mut runtime);
        assert_eq!(root.entry_count(), 4);
        assert_eq!(root.definitions_only_count(), 1);
        assert_eq!(root.unsupported_count(), 1);

        let after_paragraph = root
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(2, 1, SourceBoundaryAffinity::After),
            )
            .expect("after paragraph")
            .expect("blank");
        assert_eq!(
            after_paragraph.entry().kind(),
            M11BlockSequenceEntryKind::Blank
        );
        assert_eq!(after_paragraph.byte_range(), 2..3);
        assert_eq!(after_paragraph.utf16_range(), 1..2);

        let definitions_start = root
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(3, 2, SourceBoundaryAffinity::After),
            )
            .expect("definitions start")
            .expect("definitions");
        assert_eq!(
            definitions_start.entry().kind(),
            M11BlockSequenceEntryKind::DefinitionsOnly
        );
        assert_eq!(definitions_start.byte_range(), 3..10);
        assert_eq!(definitions_start.utf16_range(), 2..9);

        let unsupported_start = root
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(10, 9, SourceBoundaryAffinity::After),
            )
            .expect("unsupported start")
            .expect("unsupported");
        assert_eq!(
            unsupported_start.entry().kind(),
            M11BlockSequenceEntryKind::Unsupported
        );
        assert_eq!(
            unsupported_start
                .entry()
                .unsupported_reason()
                .expect("reason")
                .get(),
            7
        );
        assert!(matches!(
            root.locate_point(
                &runtime,
                M11BlockSequencePoint::new(1, 1, SourceBoundaryAffinity::After),
            ),
            Err(M11BlockSequenceError::InvalidPoint)
        ));
        let installed_claim = PersistentM11BlockRootClaim {
            summary: root.summary,
            storage_page_count: root.page_count,
            tree_height: root.tree_height,
        };
        assert!(matches!(
            persistent_m11_block_locate_point(
                runtime.producer_arena(),
                root.tree_root_id_for_test(),
                installed_claim,
                M11BlockSequencePoint::new(2, 0, SourceBoundaryAffinity::After),
            ),
            Err(M11BlockSequenceError::InvalidPoint)
        ));

        release_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }

    #[test]
    fn lane_commitments_are_ordered_and_independent() {
        let mut runtime =
            DocumentRuntime::new("ab", DocumentRuntimeConfig::default()).expect("runtime");
        let first_lease = runtime.snapshot_current_source().expect("first source");
        let mut first_build =
            M11BlockSequenceBuild::new(&runtime, first_lease).expect("first build");
        drive_entry(
            &mut first_build,
            &mut runtime,
            M11BlockSequenceEntry::paragraph(2, 2, 0, role(b"g"), role(b"p"))
                .expect("first paragraph"),
        );
        let mut first = finish_build(first_build, &mut runtime);

        let second_lease = runtime.snapshot_current_source().expect("second source");
        let mut second_build =
            M11BlockSequenceBuild::new(&runtime, second_lease).expect("second build");
        drive_entry(
            &mut second_build,
            &mut runtime,
            M11BlockSequenceEntry::paragraph(2, 2, 0, role(b"p"), role(b"g"))
                .expect("second paragraph"),
        );
        let mut second = finish_build(second_build, &mut runtime);

        assert_ne!(first.green_commitment256(), second.green_commitment256());
        assert_ne!(
            first.projection_commitment256(),
            second.projection_commitment256()
        );
        assert_ne!(
            first.green_commitment256(),
            first.projection_commitment256()
        );

        release_root(&mut first, &mut runtime);
        release_root(&mut second, &mut runtime);
        drop(first);
        drop(second);
        close_runtime(runtime);
    }

    #[test]
    fn empty_source_uses_zero_descriptors_and_no_tree_root() {
        let mut runtime =
            DocumentRuntime::new("", DocumentRuntimeConfig::default()).expect("runtime");
        let lease = runtime.snapshot_current_source().expect("source");
        let build = M11BlockSequenceBuild::new(&runtime, lease).expect("build");
        let mut root = finish_build(build, &mut runtime);
        assert_eq!(root.entry_count(), 0);
        assert_eq!(root.storage_page_count(), 0);
        assert_eq!(root.tree_height(), 0);
        assert_eq!(root.tree_root_id_for_test(), None);
        let green = descriptor_for(root.summary, 0, 0, M11BlockRoleLane::Green);
        let projection = descriptor_for(root.summary, 0, 0, M11BlockRoleLane::Projection);
        assert_eq!(green.record_count(), 0);
        assert_eq!(green.canonical_bytes(), 0);
        assert_eq!(green.commitment256(), [0; 32]);
        let claim =
            validate_persistent_m11_block_root(runtime.producer_arena(), None, green, projection)
                .expect("empty paired descriptors");
        assert_eq!(claim.entry_count(), 0);
        assert!(root
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
            )
            .expect("empty query")
            .is_none());
        let empty_window = persistent_m11_block_locate_ordinal_window(
            runtime.producer_arena(),
            None,
            claim,
            0,
            97,
        )
        .expect("empty ordinal window");
        assert_eq!(empty_window.total_entry_count(), 0);
        assert_eq!(empty_window.start_entry_ordinal(), 0);
        assert_eq!(empty_window.next_entry_ordinal(), 0);
        assert_eq!(empty_window.start_byte_offset(), 0);
        assert_eq!(empty_window.next_byte_offset(), 0);
        assert!(empty_window.complete());
        assert_eq!(
            empty_window.receipt(),
            M11BlockSequenceOrdinalWindowReceipt::default()
        );

        release_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }

    #[test]
    fn ordinal_windows_seek_directly_across_eight_thousand_one_hundred_ninety_one_entries() {
        const ENTRIES: usize = 8191;
        const WINDOW: u32 = 97;
        let text = "x".repeat(ENTRIES);
        let mut runtime =
            DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build = M11BlockSequenceBuild::new(&runtime, lease).expect("build");
        for ordinal in 0..ENTRIES {
            let entry = if ordinal % 2 == 0 {
                M11BlockSequenceEntry::paragraph(1, 1, 0, role(b"g"), role(b"p"))
                    .expect("paragraph")
            } else {
                M11BlockSequenceEntry::blank(1, 1).expect("blank")
            };
            drive_entry(&mut build, &mut runtime, entry);
        }
        let mut root = finish_build(build, &mut runtime);
        let claim = PersistentM11BlockRootClaim {
            summary: root.summary,
            storage_page_count: root.page_count,
            tree_height: root.tree_height,
        };
        let raw_root = root.tree_root_id_for_test();
        let locate = |start| {
            persistent_m11_block_locate_ordinal_window(
                runtime.producer_arena(),
                raw_root,
                claim,
                start,
                WINDOW,
            )
            .expect("ordinal window")
        };

        let first = locate(0);
        assert_eq!(first.start_entry_ordinal(), 0);
        assert_eq!(first.next_entry_ordinal(), 97);
        assert_eq!(first.start_byte_offset(), 0);
        assert_eq!(first.start_utf16_offset(), 0);
        assert_eq!(first.next_byte_offset(), 97);
        assert_eq!(first.next_utf16_offset(), 97);
        assert!(!first.complete());

        let middle = locate(4095);
        assert_eq!(middle.start_entry_ordinal(), 4095);
        assert_eq!(middle.next_entry_ordinal(), 4192);
        assert_eq!(middle.start_byte_offset(), 4095);
        assert_eq!(middle.next_byte_offset(), 4192);
        assert!(middle.receipt().storage_pages_visited() <= 2);
        assert!(
            middle.receipt().node_headers_decoded() < 128,
            "ordinal routing must stay logarithmic: {:?}",
            middle.receipt()
        );
        assert!(
            middle.receipt().packed_entries_inspected() <= 896,
            "only two bounded packed boundary pages may be inspected: {:?}",
            middle.receipt()
        );
        assert!(
            middle.receipt().node_headers_decoded() < ENTRIES as u64
                && middle.receipt().packed_entries_inspected() < ENTRIES as u64
        );

        let last = locate(8190);
        assert_eq!(last.start_entry_ordinal(), 8190);
        assert_eq!(last.next_entry_ordinal(), 8191);
        assert_eq!(last.start_byte_offset(), 8190);
        assert_eq!(last.next_byte_offset(), 8191);
        assert!(last.complete());

        let terminal = locate(8191);
        assert_eq!(terminal.start_entry_ordinal(), 8191);
        assert_eq!(terminal.next_entry_ordinal(), 8191);
        assert_eq!(terminal.start_byte_offset(), 8191);
        assert_eq!(terminal.next_byte_offset(), 8191);
        assert!(terminal.complete());
        assert_eq!(
            terminal.receipt(),
            M11BlockSequenceOrdinalWindowReceipt::default()
        );
        assert!(matches!(
            persistent_m11_block_locate_ordinal_window(
                runtime.producer_arena(),
                raw_root,
                claim,
                8192,
                WINDOW,
            ),
            Err(M11BlockSequenceError::InvalidPoint)
        ));

        release_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }

    #[test]
    fn semantic_splice_reuses_untouched_subtrees_in_an_eight_thousand_block_document() {
        let pairs = 4096_usize;
        let text = "x\n".repeat(pairs);
        let mut runtime =
            DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
        let lease = runtime.snapshot_current_source().expect("base source");
        let mut build = M11BlockSequenceBuild::new(&runtime, lease).expect("base build");
        for _ in 0..pairs {
            drive_entry(
                &mut build,
                &mut runtime,
                M11BlockSequenceEntry::paragraph(1, 1, 0, role(b"g"), role(b"p"))
                    .expect("paragraph"),
            );
            drive_entry(
                &mut build,
                &mut runtime,
                M11BlockSequenceEntry::blank(1, 1).expect("blank"),
            );
        }
        let mut base = finish_build(build, &mut runtime);
        assert_eq!(base.entry_count(), 8192);
        assert_eq!(base.storage_page_count(), 128);
        let base_root = base.tree_root_id_for_test().expect("base tree");
        let (base_leaves, base_branches) =
            reachable_block_tree_ids(runtime.producer_arena(), base_root);
        assert_eq!(base_leaves.len(), 128);
        assert_eq!(base_branches.len(), 127);

        let edited_ordinal = 4096_u64;
        runtime
            .apply_edit(
                base.source(),
                usize::try_from(edited_ordinal).expect("test ordinal")
                    ..usize::try_from(edited_ordinal + 1).expect("test ordinal"),
                "y",
            )
            .expect("same-width local source edit");
        let target_lease = runtime.snapshot_current_source().expect("target source");
        let replacement =
            [
                M11BlockSequenceEntry::paragraph(1, 1, 0, role(b"g-new"), role(b"p-new"))
                    .expect("replacement paragraph"),
            ];
        let (mut target, receipt) = splice_m11_block_sequence_atomic(
            &mut runtime,
            &base,
            target_lease,
            edited_ordinal..edited_ordinal + 1,
            &replacement,
        )
        .expect("persistent local block splice");

        assert_eq!(target.source(), runtime.current_source_version().unwrap());
        assert_eq!(target.entry_count(), 8192);
        assert_eq!(target.source_byte_len(), 8192);
        assert_eq!(receipt.base_entries(), 8192);
        assert_eq!(receipt.deleted_entries(), 1);
        assert_eq!(receipt.replacement_entries(), 1);
        assert_eq!(receipt.unchanged_entries_preserved(), 8191);
        assert_eq!(receipt.base_storage_pages(), 128);
        assert_eq!(receipt.deleted_storage_pages(), 1);
        assert_eq!(receipt.replacement_storage_pages(), 1);
        assert_eq!(receipt.reused_storage_pages(), 127);
        assert_eq!(receipt.boundary_entries_decoded(), 64);
        assert_eq!(receipt.boundary_entries_reencoded(), 63);
        assert!(
            receipt.tree_nodes_visited() < usize::from(base.tree_height()) * 16 + 32,
            "tree work must track AVL height, got {receipt:?}"
        );
        assert!(
            receipt.branches_allocated() < usize::from(base.tree_height()) * 8 + 32,
            "branch allocation must track AVL height, got {receipt:?}"
        );

        let target_root = target.tree_root_id_for_test().expect("target tree");
        assert_ne!(target_root, base_root);
        let (target_leaves, target_branches) =
            reachable_block_tree_ids(runtime.producer_arena(), target_root);
        let shared_leaves = base_leaves.intersection(&target_leaves).count();
        let shared_branches = base_branches.intersection(&target_branches).count();
        assert_eq!(
            shared_leaves,
            usize::try_from(receipt.reused_storage_pages()).expect("test page count")
        );
        assert!(
            shared_branches * 4 > base_branches.len() * 3,
            "a local edit should preserve most internal subtrees: \
             shared={shared_branches}, base={}",
            base_branches.len()
        );

        let edited = target
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(
                    usize::try_from(edited_ordinal).expect("offset"),
                    usize::try_from(edited_ordinal).expect("offset"),
                    SourceBoundaryAffinity::After,
                ),
            )
            .expect("edited lookup")
            .expect("edited entry");
        assert_eq!(edited.entry_ordinal(), edited_ordinal);
        assert_eq!(edited.byte_range(), edited_ordinal..edited_ordinal + 1);
        assert_eq!(
            edited
                .entry()
                .green()
                .expect("replacement Green")
                .as_bytes(),
            b"g-new"
        );
        assert_ne!(target.green_commitment256(), base.green_commitment256());

        release_root(&mut target, &mut runtime);
        release_root(&mut base, &mut runtime);
        drop(target);
        drop(base);
        close_runtime(runtime);
    }

    #[test]
    fn semantic_splice_rebuilds_only_two_boundary_pages_across_a_page_range() {
        let entry_count = 256_usize;
        let text = "x".repeat(entry_count);
        let mut runtime =
            DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build = M11BlockSequenceBuild::new(&runtime, lease).expect("build");
        for _ in 0..entry_count {
            drive_entry(
                &mut build,
                &mut runtime,
                M11BlockSequenceEntry::paragraph(1, 1, 0, role(b"g"), role(b"p"))
                    .expect("paragraph"),
            );
        }
        let mut base = finish_build(build, &mut runtime);
        assert_eq!(base.storage_page_count(), 4);
        let target_lease = runtime.snapshot_current_source().expect("target source");
        let replacement = [M11BlockSequenceEntry::paragraph(
            5,
            5,
            0,
            role(b"joined"),
            role(b"joined-projection"),
        )
        .expect("joined paragraph")];
        let (mut target, receipt) = splice_m11_block_sequence_atomic(
            &mut runtime,
            &base,
            target_lease,
            62..67,
            &replacement,
        )
        .expect("cross-page semantic splice");

        assert_eq!(target.entry_count(), 252);
        assert_eq!(target.source_byte_len(), 256);
        assert_eq!(receipt.deleted_entries(), 5);
        assert_eq!(receipt.replacement_entries(), 1);
        assert_eq!(receipt.deleted_storage_pages(), 2);
        assert_eq!(receipt.reused_storage_pages(), 2);
        assert_eq!(receipt.boundary_entries_decoded(), 128);
        assert_eq!(receipt.boundary_entries_reencoded(), 123);

        let joined = target
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(62, 62, SourceBoundaryAffinity::After),
            )
            .expect("joined lookup")
            .expect("joined block");
        assert_eq!(joined.entry_ordinal(), 62);
        assert_eq!(joined.byte_range(), 62..67);
        assert_eq!(
            joined.entry().green().expect("joined Green").as_bytes(),
            b"joined"
        );
        let suffix = target
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(67, 67, SourceBoundaryAffinity::After),
            )
            .expect("suffix lookup")
            .expect("suffix block");
        assert_eq!(suffix.entry_ordinal(), 63);
        assert_eq!(suffix.byte_range(), 67..68);
        assert_eq!(
            suffix.entry().green().expect("suffix Green").as_bytes(),
            b"g"
        );

        let target_claim = PersistentM11BlockRootClaim {
            summary: target.summary,
            storage_page_count: target.page_count,
            tree_height: target.tree_height,
        };
        let mut underfilled_ordinals = Vec::new();
        let underfilled = persistent_m11_block_visit_entries(
            runtime.producer_arena(),
            target.tree_root_id_for_test(),
            target_claim,
            M11BlockSequenceVisitStart {
                entry_ordinal: 64,
                byte_offset: 68,
                utf16_offset: 68,
            },
            100,
            1,
            |entry| {
                underfilled_ordinals.push(entry.entry_ordinal());
                M11BlockSequenceVisitControl::Continue
            },
        )
        .expect("underfilled replacement-page visit");
        assert_eq!(underfilled_ordinals.first(), Some(&64));
        assert_eq!(underfilled_ordinals.last(), Some(&123));
        assert_eq!(underfilled.visited_entries(), 60);
        assert_eq!(underfilled.storage_pages_visited(), 1);
        assert_eq!(underfilled.next_entry_ordinal(), 124);
        assert_eq!(underfilled.next_byte_offset(), 128);
        assert_eq!(
            underfilled.disposition(),
            M11BlockSequenceVisitDisposition::StoragePageLimit
        );

        release_root(&mut target, &mut runtime);
        release_root(&mut base, &mut runtime);
        drop(target);
        drop(base);
        close_runtime(runtime);
    }

    #[test]
    fn publication_retain_is_additional_and_root_release_is_explicit() {
        let mut runtime =
            DocumentRuntime::new("x", DocumentRuntimeConfig::default()).expect("runtime");
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build = M11BlockSequenceBuild::new(&runtime, lease).expect("build");
        drive_entry(
            &mut build,
            &mut runtime,
            M11BlockSequenceEntry::paragraph(1, 1, 0, role(b"g"), role(b"p")).expect("paragraph"),
        );
        let mut root = finish_build(build, &mut runtime);
        let root_id = root.tree_root_id_for_test().expect("tree root");
        let runtime_identity = runtime.producer_identity();
        let source = root.source();
        {
            let mut session = runtime.producer_arena_mut().begin_build().expect("journal");
            let mut retained = root
                .retain_for_publication(&mut session, runtime_identity, source)
                .expect("retain");
            assert_eq!(retained.entry_count(), 1);
            assert_eq!(retained.page_count(), 1);
            assert_eq!(retained.tree_height(), 1);
            assert!(retained.inspection().node_headers_decoded > 0);
            let green = retained.descriptor(M11BlockRoleLane::Green);
            let projection = retained.descriptor(M11BlockRoleLane::Projection);
            let green_bytes =
                encode_persistent_m11_block_role_descriptor(green).expect("Green descriptor");
            assert_eq!(
                decode_persistent_m11_block_role_descriptor(
                    &green_bytes,
                    M11BlockRoleLane::Green,
                    1,
                    1,
                )
                .expect("decode Green descriptor"),
                green
            );
            let mut corrupt = green_bytes;
            corrupt[0] ^= 0xff;
            assert!(decode_persistent_m11_block_role_descriptor(
                &corrupt,
                M11BlockRoleLane::Green,
                1,
                1,
            )
            .is_err());
            let owner = retained.take_owner().expect("retained owner");
            assert_eq!(owner.id(), root_id);
            let claim = validate_persistent_m11_block_root(
                session.arena(),
                Some(owner.id()),
                green,
                projection,
            )
            .expect("paired descriptors");
            assert_eq!(claim.entry_count(), 1);
            session.release(owner).expect("release retained owner");
            // Dropping an active empty session schedules the journal abort.
        }
        while runtime.arena_metrics().pending_build_aborts > 0
            || runtime.arena_metrics().pending_reclaims > 0
        {
            runtime.producer_arena_mut().poll_reclaim(64);
        }
        let located = root
            .locate_point(
                &runtime,
                M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
            )
            .expect("query after retain")
            .expect("retained entry");
        let leaf_payload_bytes =
            BLOCK_LEAF_HEADER_BYTES + BLOCK_ENTRY_HEADER_BYTES + b"g".len() + b"p".len();
        assert_eq!(
            located.receipt().payload_bytes_inspected(),
            u64::try_from(leaf_payload_bytes * 5).expect("test payload fits"),
            "the receipt includes all routing/header decodes and the final packed-leaf decode"
        );

        release_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }
}
