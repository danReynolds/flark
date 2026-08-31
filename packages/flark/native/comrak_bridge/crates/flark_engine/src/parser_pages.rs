//! Opaque fixed-page storage for parser-owned event and byte streams.
//!
//! Markdown semantics stay in `flark-parser`. This module owns only exact
//! source binding, bounded record copies, arena journals, persistent measured
//! page roots, source-order replay, and explicit cancellation/reclamation.

use std::fmt;
use std::ops::Range;

use crate::document::DocumentRuntime;
use crate::identity::ArenaId;
use crate::identity::RuntimeIdentity;
use crate::measured_sequence::{
    begin_measured_sequence_seal, retain_committed_measured_sequence_root_with_measure,
    validate_measured_sequence_node, BeginMeasuredSequenceSealFailure,
    CommittedMeasuredSequenceRoot, MeasuredSequenceBuildRoot, MeasuredSequenceRef,
    MeasuredSequenceSeal, ResumableMeasuredSequenceBuilder, ResumableSequenceProgress,
    SequenceInspectionReceipt, SequenceMeasure, SequenceMutationReceipt, SequenceSpec,
    SequenceSpecInspection,
};
use crate::source::{SourceCursor, SourceEditError, SourceSnapshotLease, SourceVersion};
use crate::storage::{ArenaBuildOwner, ArenaBuildSession, ArenaError, CandidateBuild, PageArena};
use crate::{ReclaimReceipt, ARENA_PAGE_BYTES};

const PAGE_LEAF_MAGIC: [u8; 4] = *b"PPL1";
const PAGE_BRANCH_MAGIC: [u8; 4] = *b"PPB1";
const PAGE_SCHEMA: u32 = 1;
const PAGE_LEAF_HEADER_BYTES: usize = 20;
const PAGE_BRANCH_BYTES: usize = 110;
const COMMITMENT_LANES: usize = 4;
const COMMITMENT_BASES: [u64; COMMITMENT_LANES] = [
    0x0000_0100_0000_01b3,
    0x9e37_79b1_85eb_ca87,
    0xc2b2_ae3d_27d4_eb4f,
    0x1656_67b1_9e37_79f9,
];

/// Maximum caller-defined bytes in one generic parser record.
pub const M11_PARSER_PAGE_MAX_RECORD_BYTES: usize = 256;
/// Maximum source bytes copied by one range-cursor poll.
pub const M11_PARSER_RANGE_MAX_POLL_BYTES: usize = ARENA_PAGE_BYTES;
/// Maximum builder or reclamation transitions accepted in one poll.
pub const M11_PARSER_PAGE_MAX_POLL_TRANSITIONS: usize = 4096;

fn validate_source_range_authority(
    runtime: &DocumentRuntime,
    lease: &SourceSnapshotLease,
    source_range: &Range<usize>,
) -> Result<(RuntimeIdentity, SourceVersion), M11ParserPageError> {
    let source = lease.version();
    if runtime.current_source_version() != Some(source) {
        return Err(M11ParserPageError::SourceAuthorityMismatch);
    }
    if source_range.start > source_range.end
        || source_range.end > source.byte_len()
        || u32::try_from(source_range.end).is_err()
        || lease.utf16_offset_for_byte(source_range.start).is_err()
        || lease.utf16_offset_for_byte(source_range.end).is_err()
    {
        return Err(M11ParserPageError::InvalidRange);
    }
    Ok((runtime.producer_identity(), source))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PageCommitment {
    hash: [u64; COMMITMENT_LANES],
    factor: [u64; COMMITMENT_LANES],
}

impl PageCommitment {
    const fn empty() -> Self {
        Self {
            hash: [0; COMMITMENT_LANES],
            factor: [1; COMMITMENT_LANES],
        }
    }

    fn for_bytes(bytes: &[u8]) -> Self {
        let mut commitment = Self::empty();
        for byte in bytes {
            for (lane, base) in COMMITMENT_BASES.iter().copied().enumerate() {
                commitment.hash[lane] = commitment.hash[lane]
                    .wrapping_mul(base)
                    .wrapping_add(u64::from(*byte) + 1);
                commitment.factor[lane] = commitment.factor[lane].wrapping_mul(base);
            }
        }
        commitment
    }

    const fn combine(self, right: Self) -> Self {
        let mut hash = [0; COMMITMENT_LANES];
        let mut factor = [0; COMMITMENT_LANES];
        let mut lane = 0;
        while lane < COMMITMENT_LANES {
            hash[lane] = self.hash[lane]
                .wrapping_mul(right.factor[lane])
                .wrapping_add(right.hash[lane]);
            factor[lane] = self.factor[lane].wrapping_mul(right.factor[lane]);
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

/// Shape-independent exact summary of one generic parser page stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PageSummary {
    stream_tag: u32,
    records: u64,
    payload_bytes: u64,
    encoded_bytes: u64,
    commitment: PageCommitment,
}

impl PageSummary {
    const fn empty(stream_tag: u32) -> Self {
        Self {
            stream_tag,
            records: 0,
            payload_bytes: 0,
            encoded_bytes: 0,
            commitment: PageCommitment::empty(),
        }
    }
}

struct ParserPageSpec;

impl SequenceSpec for ParserPageSpec {
    type Summary = PageSummary;
    type Error = M11ParserPageError;

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
        if payload.get(..4) != Some(PAGE_BRANCH_MAGIC.as_slice()) {
            return Ok(None);
        }
        if payload.len() != PAGE_BRANCH_BYTES {
            return Err(M11ParserPageError::Corrupt(
                "parser page branch has the wrong length",
            ));
        }
        inspection
            .charge_payload_bytes(payload.len())
            .ok_or(M11ParserPageError::CounterOverflow)?;
        let mut cursor = 4;
        let schema = read_u32(payload, &mut cursor)?;
        if schema != PAGE_SCHEMA {
            return Err(M11ParserPageError::Corrupt(
                "parser page branch schema is unsupported",
            ));
        }
        let stream_tag = read_u32(payload, &mut cursor)?;
        let leaves = read_u64(payload, &mut cursor)?;
        let height = read_u16(payload, &mut cursor)?;
        let records = read_u64(payload, &mut cursor)?;
        let payload_bytes = read_u64(payload, &mut cursor)?;
        let encoded_bytes = read_u64(payload, &mut cursor)?;
        let commitment = decode_commitment(payload, &mut cursor)?;
        if cursor != payload.len() || stream_tag == 0 || leaves < 2 || height < 2 {
            return Err(M11ParserPageError::Corrupt(
                "parser page branch metadata is invalid",
            ));
        }
        Ok(Some(SequenceMeasure::new(
            PageSummary {
                stream_tag,
                records,
                payload_bytes,
                encoded_bytes,
                commitment,
            },
            leaves,
            height,
        )))
    }

    fn encode_branch(
        measure: SequenceMeasure<Self::Summary>,
        output: &mut [u8; ARENA_PAGE_BYTES],
    ) -> Result<usize, Self::Error> {
        let summary = measure.summary();
        if summary.stream_tag == 0 || measure.leaves() < 2 || measure.height() < 2 {
            return Err(M11ParserPageError::Corrupt(
                "parser page branch measure is invalid",
            ));
        }
        let mut cursor = 0;
        write_bytes(output, &mut cursor, &PAGE_BRANCH_MAGIC)?;
        write_u32(output, &mut cursor, PAGE_SCHEMA)?;
        write_u32(output, &mut cursor, summary.stream_tag)?;
        write_u64(output, &mut cursor, measure.leaves())?;
        write_u16(output, &mut cursor, measure.height())?;
        write_u64(output, &mut cursor, summary.records)?;
        write_u64(output, &mut cursor, summary.payload_bytes)?;
        write_u64(output, &mut cursor, summary.encoded_bytes)?;
        encode_commitment(summary.commitment, output, &mut cursor)?;
        if cursor != PAGE_BRANCH_BYTES {
            return Err(M11ParserPageError::Corrupt(
                "parser page branch encoding length changed",
            ));
        }
        Ok(cursor)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        if left.stream_tag == 0 || left.stream_tag != right.stream_tag {
            return Err(M11ParserPageError::Corrupt(
                "adjacent parser page streams use different tags",
            ));
        }
        Ok(PageSummary {
            stream_tag: left.stream_tag,
            records: left
                .records
                .checked_add(right.records)
                .ok_or(M11ParserPageError::CounterOverflow)?,
            payload_bytes: left
                .payload_bytes
                .checked_add(right.payload_bytes)
                .ok_or(M11ParserPageError::CounterOverflow)?,
            encoded_bytes: left
                .encoded_bytes
                .checked_add(right.encoded_bytes)
                .ok_or(M11ParserPageError::CounterOverflow)?,
            commitment: left.commitment.combine(right.commitment),
        })
    }

    fn invalid(message: &'static str) -> Self::Error {
        M11ParserPageError::Corrupt(message)
    }
}

type ParserPageBuilder = ResumableMeasuredSequenceBuilder<ParserPageSpec>;
type ParserPageBuildRoot = MeasuredSequenceBuildRoot<ParserPageSpec>;
type ParserPageSeal = MeasuredSequenceSeal<ParserPageSpec>;
type ParserPageTree = CommittedMeasuredSequenceRoot<ParserPageSpec>;

/// One journalled retain of a generic parser-page root.
///
/// This is deliberately crate-private: typed parser facades may bind the
/// generic stream into a role descriptor, but no endpoint can turn an arena
/// identifier into parser authority.
pub(crate) struct M11RetainedParserPageRoot {
    owner: Option<ArenaBuildOwner>,
}

impl M11RetainedParserPageRoot {
    pub(crate) fn take_owner(&mut self) -> Option<ArenaBuildOwner> {
        self.owner.take()
    }
}

/// Authenticated generic summary required to reopen an imported page root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11ImportedParserPageRootClaim {
    pub(crate) stream_tag: u32,
    pub(crate) storage_page_count: u64,
    pub(crate) record_count: u64,
    pub(crate) payload_bytes: u64,
    pub(crate) encoded_bytes: u64,
    pub(crate) checksum: [u8; 32],
}

/// Authority, lifecycle, resource, or canonical-page failure.
#[derive(Debug)]
pub enum M11ParserPageError {
    InvalidStreamTag,
    InvalidRange,
    SourceAuthorityMismatch,
    RecordEmpty,
    RecordTooLarge { bytes: usize, cap: usize },
    RecordAlreadyPending,
    InputClosed,
    InvalidState,
    WrongRuntime,
    ZeroFuel,
    PollLimitExceeded,
    OutputTooLarge,
    CounterOverflow,
    Corrupt(&'static str),
    Arena(ArenaError),
    Source(SourceEditError),
}

impl fmt::Display for M11ParserPageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStreamTag => formatter.write_str("parser page stream tag must be nonzero"),
            Self::InvalidRange => formatter.write_str("parser page source range is invalid"),
            Self::SourceAuthorityMismatch => {
                formatter.write_str("parser page source authority is not current")
            }
            Self::RecordEmpty => formatter.write_str("parser page records must not be empty"),
            Self::RecordTooLarge { bytes, cap } => {
                write!(
                    formatter,
                    "parser page record has {bytes} bytes above the {cap}-byte cap"
                )
            }
            Self::RecordAlreadyPending => {
                formatter.write_str("a parser page record is already pending")
            }
            Self::InputClosed => formatter.write_str("parser page input is already closed"),
            Self::InvalidState => formatter.write_str("parser page owner is in the wrong state"),
            Self::WrongRuntime => {
                formatter.write_str("parser page owner belongs to another document runtime")
            }
            Self::ZeroFuel => formatter.write_str("parser page poll requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("parser page poll exceeds the bounded transition limit")
            }
            Self::OutputTooLarge => {
                formatter.write_str("parser range output exceeds the bounded copy limit")
            }
            Self::CounterOverflow => formatter.write_str("parser page counter overflow"),
            Self::Corrupt(message) => write!(formatter, "corrupt parser page stream: {message}"),
            Self::Arena(error) => write!(formatter, "parser page arena failure: {error}"),
            Self::Source(error) => write!(formatter, "parser page source failure: {error}"),
        }
    }
}

impl std::error::Error for M11ParserPageError {}

impl From<ArenaError> for M11ParserPageError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<SourceEditError> for M11ParserPageError {
    fn from(value: SourceEditError) -> Self {
        Self::Source(value)
    }
}

/// Fixed-capacity value offered to or returned from a generic page stream.
pub struct M11ParserPageRecord {
    bytes: [u8; M11_PARSER_PAGE_MAX_RECORD_BYTES],
    len: u16,
}

impl fmt::Debug for M11ParserPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ParserPageRecord")
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl M11ParserPageRecord {
    pub fn new(bytes: &[u8]) -> Result<Self, M11ParserPageError> {
        if bytes.is_empty() {
            return Err(M11ParserPageError::RecordEmpty);
        }
        if bytes.len() > M11_PARSER_PAGE_MAX_RECORD_BYTES {
            return Err(M11ParserPageError::RecordTooLarge {
                bytes: bytes.len(),
                cap: M11_PARSER_PAGE_MAX_RECORD_BYTES,
            });
        }
        let mut storage = [0_u8; M11_PARSER_PAGE_MAX_RECORD_BYTES];
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

/// Exact bounded source-window work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11ParserRangeReceipt {
    transitions: usize,
    bytes_read: usize,
    refill_count: usize,
    maximum_refill_bytes: usize,
}

impl M11ParserRangeReceipt {
    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }

    #[must_use]
    pub const fn bytes_read(self) -> usize {
        self.bytes_read
    }

    #[must_use]
    pub const fn refill_count(self) -> usize {
        self.refill_count
    }

    #[must_use]
    pub const fn maximum_refill_bytes(self) -> usize {
        self.maximum_refill_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11ParserRangeStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ParserRangePoll {
    status: M11ParserRangeStatus,
    transitions: usize,
    bytes_read: usize,
}

impl M11ParserRangePoll {
    #[must_use]
    pub const fn status(self) -> M11ParserRangeStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }

    #[must_use]
    pub const fn bytes_read(self) -> usize {
        self.bytes_read
    }
}

/// Resumable bounded-copy cursor over an exact immutable source range.
pub struct M11ParserRangeCursor {
    cursor: Option<SourceCursor>,
    receipt: M11ParserRangeReceipt,
    complete: bool,
}

impl fmt::Debug for M11ParserRangeCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ParserRangeCursor")
            .field("receipt", &self.receipt)
            .field("complete", &self.complete)
            .finish_non_exhaustive()
    }
}

impl M11ParserRangeCursor {
    fn new(lease: SourceSnapshotLease, range: Range<usize>) -> Result<Self, M11ParserPageError> {
        Ok(Self {
            cursor: Some(lease.cursor_in(range)?),
            receipt: M11ParserRangeReceipt::default(),
            complete: false,
        })
    }

    pub fn poll(
        &mut self,
        fuel: usize,
        output: &mut [u8],
    ) -> Result<M11ParserRangePoll, M11ParserPageError> {
        if fuel == 0 {
            return Err(M11ParserPageError::ZeroFuel);
        }
        if fuel > M11_PARSER_RANGE_MAX_POLL_BYTES {
            return Err(M11ParserPageError::PollLimitExceeded);
        }
        if output.is_empty() || output.len() > M11_PARSER_RANGE_MAX_POLL_BYTES {
            return Err(M11ParserPageError::OutputTooLarge);
        }
        if self.complete {
            return Ok(M11ParserRangePoll {
                status: M11ParserRangeStatus::Complete,
                transitions: 0,
                bytes_read: 0,
            });
        }
        let limit = fuel.min(output.len());
        let cursor = self
            .cursor
            .as_mut()
            .ok_or(M11ParserPageError::InvalidState)?;
        let bytes_read = cursor.read(&mut output[..limit]);
        let complete = cursor.position() == cursor.end();
        self.receipt.transitions = self
            .receipt
            .transitions
            .checked_add(bytes_read)
            .ok_or(M11ParserPageError::CounterOverflow)?;
        self.receipt.bytes_read = self
            .receipt
            .bytes_read
            .checked_add(bytes_read)
            .ok_or(M11ParserPageError::CounterOverflow)?;
        self.receipt.refill_count = cursor.refill_count();
        self.receipt.maximum_refill_bytes = cursor.max_refill_bytes();
        if complete {
            let lease = self
                .cursor
                .take()
                .ok_or(M11ParserPageError::InvalidState)?
                .finish()?;
            drop(lease);
            self.complete = true;
        }
        Ok(M11ParserRangePoll {
            status: if complete {
                M11ParserRangeStatus::Complete
            } else {
                M11ParserRangeStatus::Pending
            },
            transitions: bytes_read,
            bytes_read,
        })
    }

    pub fn cancel(&mut self) {
        if let Some(cursor) = self.cursor.take() {
            drop(cursor.cancel());
        }
        self.complete = true;
    }

    #[must_use]
    pub const fn receipt(&self) -> M11ParserRangeReceipt {
        self.receipt
    }
}

impl Drop for M11ParserRangeCursor {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.cursor.is_none(),
                "parser range cursors require completion or explicit cancellation"
            );
        }
    }
}

/// Move-only authority for one scalar-aligned range of the current source.
///
/// The workspace parser can mint any number of resumable cursors, but it
/// cannot extract or replace the immutable source lease. Every cursor
/// therefore reads the exact source version and range authenticated at
/// construction. Cursor creation also rechecks the document runtime and
/// current source before duplicating the private lease.
///
/// ```compile_fail
/// fn duplicate(
///     authority: &flark_engine::parser_internal::M11ParserSourceRangeAuthority,
/// ) -> flark_engine::parser_internal::M11ParserSourceRangeAuthority {
///     authority.clone()
/// }
/// ```
///
/// ```compile_fail
/// fn extract_lease(
///     authority: &flark_engine::parser_internal::M11ParserSourceRangeAuthority,
/// ) -> flark_engine::SourceSnapshotLease {
///     authority.source_lease()
/// }
/// ```
pub struct M11ParserSourceRangeAuthority {
    runtime_identity: RuntimeIdentity,
    lease: SourceSnapshotLease,
    source: SourceVersion,
    source_range: Range<usize>,
}

impl fmt::Debug for M11ParserSourceRangeAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ParserSourceRangeAuthority")
            .field("source", &self.source)
            .field("source_range", &self.source_range)
            .finish_non_exhaustive()
    }
}

impl M11ParserSourceRangeAuthority {
    pub fn new(
        runtime: &DocumentRuntime,
        lease: SourceSnapshotLease,
        source_range: Range<usize>,
    ) -> Result<Self, M11ParserPageError> {
        let (runtime_identity, source) =
            validate_source_range_authority(runtime, &lease, &source_range)?;
        Ok(Self {
            runtime_identity,
            lease,
            source,
            source_range,
        })
    }

    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub fn source_range(&self) -> Range<usize> {
        self.source_range.clone()
    }

    /// Revalidates that this authority still belongs to the supplied open
    /// document actor and remains its exact current source.
    pub fn validate(&self, runtime: &DocumentRuntime) -> Result<(), M11ParserPageError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ParserPageError::WrongRuntime);
        }
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11ParserPageError::SourceAuthorityMismatch);
        }
        Ok(())
    }

    pub fn cursor(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<M11ParserRangeCursor, M11ParserPageError> {
        self.validate(runtime)?;
        M11ParserRangeCursor::new(self.lease.duplicate(), self.source_range.clone())
    }

    /// Consumes parser-internal range authority into its exact immutable
    /// source lease.
    ///
    /// The move preserves uniqueness: callers cannot retain the authority and
    /// independently widen or replay the lease. This is used by parser-owned
    /// local-delta planning before the document advances to a target revision.
    #[doc(hidden)]
    #[must_use]
    pub fn into_source_lease(self) -> SourceSnapshotLease {
        self.lease
    }
}

/// Public cumulative work for one persistent parser page build.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11ParserPageBuildReceipt {
    transitions: usize,
    records: u64,
    payload_bytes: u64,
    maximum_record_copy_bytes: usize,
    leaves_adopted: usize,
    branches_allocated: usize,
    branch_payload_bytes: usize,
    node_headers_decoded: u64,
    payload_bytes_inspected: u64,
    items_hashed: u64,
    maximum_live_bins: usize,
    reserved_scratch_bytes: usize,
    seal_transitions: usize,
}

impl M11ParserPageBuildReceipt {
    fn from_mutation(
        transitions: usize,
        records: u64,
        payload_bytes: u64,
        maximum_record_copy_bytes: usize,
        mutation: SequenceMutationReceipt,
        seal_transitions: usize,
    ) -> Self {
        Self {
            transitions,
            records,
            payload_bytes,
            maximum_record_copy_bytes,
            leaves_adopted: mutation.leaves_adopted,
            branches_allocated: mutation.branches_allocated,
            branch_payload_bytes: mutation.branch_payload_bytes,
            node_headers_decoded: mutation.inspection.node_headers_decoded,
            payload_bytes_inspected: mutation.inspection.spec.payload_bytes_inspected,
            items_hashed: mutation.inspection.spec.spec_items_hashed,
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
    pub const fn records(self) -> u64 {
        self.records
    }

    #[must_use]
    pub const fn payload_bytes(self) -> u64 {
        self.payload_bytes
    }

    #[must_use]
    pub const fn maximum_record_copy_bytes(self) -> usize {
        self.maximum_record_copy_bytes
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
    pub const fn items_hashed(self) -> u64 {
        self.items_hashed
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11ParserPageBuildStatus {
    NeedsInput,
    Pending,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ParserPageBuildPoll {
    status: M11ParserPageBuildStatus,
    transitions: usize,
}

impl M11ParserPageBuildPoll {
    #[must_use]
    pub const fn status(self) -> M11ParserPageBuildStatus {
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

/// Resumable builder for one exact-source-bound stream of opaque records.
#[must_use = "parser page builds require completion or explicit cancellation"]
pub struct M11ParserPageBuild {
    runtime_identity: RuntimeIdentity,
    lease: Option<SourceSnapshotLease>,
    source: SourceVersion,
    source_range: Range<usize>,
    stream_tag: u32,
    phase: BuildPhase,
    input_closed: bool,
    pending_record: Option<M11ParserPageRecord>,
    page: [u8; ARENA_PAGE_BYTES],
    page_len: usize,
    page_records: u16,
    page_payload_bytes: u16,
    builder: Option<ParserPageBuilder>,
    build: Option<CandidateBuild>,
    build_root: Option<ParserPageBuildRoot>,
    seal: Option<ParserPageSeal>,
    failed_tree: Option<ParserPageTree>,
    output: Option<M11ParserPageRoot>,
    mutation: SequenceMutationReceipt,
    transitions: usize,
    records: u64,
    payload_bytes: u64,
    expected_encoded_bytes: u64,
    expected_commitment: PageCommitment,
    maximum_record_copy_bytes: usize,
    seal_transitions: usize,
}

impl fmt::Debug for M11ParserPageBuild {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ParserPageBuild")
            .field("source", &self.source)
            .field("source_range", &self.source_range)
            .field("stream_tag", &self.stream_tag)
            .field("phase", &self.phase)
            .field("receipt", &self.receipt())
            .finish_non_exhaustive()
    }
}

impl M11ParserPageBuild {
    pub fn new(
        runtime: &DocumentRuntime,
        lease: SourceSnapshotLease,
        source_range: Range<usize>,
        stream_tag: u32,
    ) -> Result<Self, M11ParserPageError> {
        if stream_tag == 0 {
            return Err(M11ParserPageError::InvalidStreamTag);
        }
        let (runtime_identity, source) =
            validate_source_range_authority(runtime, &lease, &source_range)?;
        Ok(Self {
            runtime_identity,
            lease: Some(lease),
            source,
            source_range,
            stream_tag,
            phase: BuildPhase::Accepting,
            input_closed: false,
            pending_record: None,
            page: [0; ARENA_PAGE_BYTES],
            page_len: PAGE_LEAF_HEADER_BYTES,
            page_records: 0,
            page_payload_bytes: 0,
            builder: None,
            build: None,
            build_root: None,
            seal: None,
            failed_tree: None,
            output: None,
            mutation: SequenceMutationReceipt::default(),
            transitions: 0,
            records: 0,
            payload_bytes: 0,
            expected_encoded_bytes: 0,
            expected_commitment: PageCommitment::empty(),
            maximum_record_copy_bytes: 0,
            seal_transitions: 0,
        })
    }

    pub(crate) fn new_from_source_authority(
        runtime: &DocumentRuntime,
        authority: &M11ParserSourceRangeAuthority,
        stream_tag: u32,
    ) -> Result<Self, M11ParserPageError> {
        authority.validate(runtime)?;
        Self::new(
            runtime,
            authority.lease.duplicate(),
            authority.source_range.clone(),
            stream_tag,
        )
    }

    pub fn new_from_root(
        runtime: &DocumentRuntime,
        root: &M11ParserPageRoot,
        stream_tag: u32,
    ) -> Result<Self, M11ParserPageError> {
        root.ensure_runtime(runtime)?;
        let lease = root
            .lease
            .as_ref()
            .ok_or(M11ParserPageError::InvalidState)?
            .duplicate();
        Self::new(runtime, lease, root.source_range.clone(), stream_tag)
    }

    pub fn source_cursor(&self) -> Result<M11ParserRangeCursor, M11ParserPageError> {
        let lease = self
            .lease
            .as_ref()
            .ok_or(M11ParserPageError::InvalidState)?
            .duplicate();
        M11ParserRangeCursor::new(lease, self.source_range.clone())
    }

    pub fn offer_record(&mut self, record: M11ParserPageRecord) -> Result<(), M11ParserPageError> {
        if self.input_closed {
            return Err(M11ParserPageError::InputClosed);
        }
        if self.phase != BuildPhase::Accepting {
            return Err(M11ParserPageError::InvalidState);
        }
        if self.pending_record.is_some() {
            return Err(M11ParserPageError::RecordAlreadyPending);
        }
        self.pending_record = Some(record);
        Ok(())
    }

    pub fn finish_input(&mut self) -> Result<(), M11ParserPageError> {
        if self.input_closed {
            return Err(M11ParserPageError::InputClosed);
        }
        if self.phase != BuildPhase::Accepting || self.pending_record.is_some() {
            return Err(M11ParserPageError::InvalidState);
        }
        self.input_closed = true;
        Ok(())
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ParserPageBuildPoll, M11ParserPageError> {
        self.ensure_runtime(runtime)?;
        validate_fuel(fuel)?;
        let before = self.transitions;
        while self.transitions - before < fuel {
            match self.phase {
                BuildPhase::Accepting if self.pending_record.is_none() && !self.input_closed => {
                    return Ok(M11ParserPageBuildPoll {
                        status: M11ParserPageBuildStatus::NeedsInput,
                        transitions: self.transitions - before,
                    });
                }
                BuildPhase::Complete => {
                    return Ok(M11ParserPageBuildPoll {
                        status: M11ParserPageBuildStatus::Complete,
                        transitions: self.transitions - before,
                    });
                }
                BuildPhase::Cancelled => {
                    return Ok(M11ParserPageBuildPoll {
                        status: M11ParserPageBuildStatus::Cancelled,
                        transitions: self.transitions - before,
                    });
                }
                BuildPhase::Failed => return Err(M11ParserPageError::InvalidState),
                _ => self.step(runtime)?,
            }
            self.transitions = self
                .transitions
                .checked_add(1)
                .ok_or(M11ParserPageError::CounterOverflow)?;
        }
        Ok(M11ParserPageBuildPoll {
            status: match self.phase {
                BuildPhase::Complete => M11ParserPageBuildStatus::Complete,
                BuildPhase::Cancelled => M11ParserPageBuildStatus::Cancelled,
                BuildPhase::Accepting if self.pending_record.is_none() && !self.input_closed => {
                    M11ParserPageBuildStatus::NeedsInput
                }
                _ => M11ParserPageBuildStatus::Pending,
            },
            transitions: self.transitions - before,
        })
    }

    fn step(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11ParserPageError> {
        match self.phase {
            BuildPhase::Accepting => self.step_accepting(runtime),
            BuildPhase::Pushing => self.poll_push(runtime),
            BuildPhase::ReadyForFinish => self.begin_finish(runtime),
            BuildPhase::Finishing => self.poll_finish(runtime),
            BuildPhase::ReadyForRoot => self.take_build_root(runtime),
            BuildPhase::ReadyForSeal => self.begin_seal(runtime),
            BuildPhase::Sealing => self.poll_seal(runtime),
            BuildPhase::Complete | BuildPhase::Cancelled | BuildPhase::Failed => {
                Err(M11ParserPageError::InvalidState)
            }
        }
    }

    fn step_accepting(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11ParserPageError> {
        if let Some(record) = self.pending_record.as_ref() {
            let encoded = 2 + record.as_bytes().len();
            if self.page_records > 0 && self.page_len + encoded > ARENA_PAGE_BYTES {
                return self.begin_page(runtime);
            }
            if self.page_len + encoded > ARENA_PAGE_BYTES {
                self.phase = BuildPhase::Failed;
                return Err(M11ParserPageError::RecordTooLarge {
                    bytes: record.as_bytes().len(),
                    cap: M11_PARSER_PAGE_MAX_RECORD_BYTES,
                });
            }
            let bytes = record.as_bytes();
            let len = u16::try_from(bytes.len()).expect("record cap fits u16");
            let next_page_records = self
                .page_records
                .checked_add(1)
                .ok_or(M11ParserPageError::CounterOverflow)?;
            let next_page_payload_bytes = self
                .page_payload_bytes
                .checked_add(len)
                .ok_or(M11ParserPageError::CounterOverflow)?;
            let next_records = self
                .records
                .checked_add(1)
                .ok_or(M11ParserPageError::CounterOverflow)?;
            let next_payload_bytes = self
                .payload_bytes
                .checked_add(u64::from(len))
                .ok_or(M11ParserPageError::CounterOverflow)?;
            let next_encoded_bytes = self
                .expected_encoded_bytes
                .checked_add(
                    u64::try_from(encoded).map_err(|_| M11ParserPageError::CounterOverflow)?,
                )
                .ok_or(M11ParserPageError::CounterOverflow)?;
            let record_commitment = PageCommitment::for_bytes(&len.to_le_bytes())
                .combine(PageCommitment::for_bytes(bytes));
            let next_commitment = self.expected_commitment.combine(record_commitment);
            let record = self
                .pending_record
                .take()
                .ok_or(M11ParserPageError::InvalidState)?;
            let bytes = record.as_bytes();
            self.page[self.page_len..self.page_len + 2].copy_from_slice(&len.to_le_bytes());
            self.page_len += 2;
            self.page[self.page_len..self.page_len + bytes.len()].copy_from_slice(bytes);
            self.page_len += bytes.len();
            self.page_records = next_page_records;
            self.page_payload_bytes = next_page_payload_bytes;
            self.records = next_records;
            self.payload_bytes = next_payload_bytes;
            self.expected_encoded_bytes = next_encoded_bytes;
            self.expected_commitment = next_commitment;
            self.maximum_record_copy_bytes = self.maximum_record_copy_bytes.max(bytes.len());
            return Ok(());
        }
        if !self.input_closed {
            return Err(M11ParserPageError::InvalidState);
        }
        if self.page_records > 0 {
            return self.begin_page(runtime);
        }
        if self.builder.is_some() {
            self.phase = BuildPhase::ReadyForFinish;
            return Ok(());
        }
        let lease = self.lease.take().ok_or(M11ParserPageError::InvalidState)?;
        self.output = Some(M11ParserPageRoot::empty(
            self.runtime_identity,
            lease,
            self.source_range.clone(),
            self.stream_tag,
            self.receipt(),
        ));
        self.phase = BuildPhase::Complete;
        Ok(())
    }

    fn begin_page(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11ParserPageError> {
        encode_leaf_header(
            &mut self.page,
            self.stream_tag,
            self.page_records,
            self.page_len - PAGE_LEAF_HEADER_BYTES,
            usize::from(self.page_payload_bytes),
        )?;
        let payload_len = self.page_len;
        if self.builder.is_none() {
            let result = (|| {
                let mut session = runtime.producer_arena_mut().begin_build()?;
                let mut builder = ParserPageBuilder::try_new(&mut session, &mut self.mutation)?;
                let leaf = session.allocate(&self.page[..payload_len], &[])?;
                builder.begin_push(&session, leaf, &mut self.mutation)?;
                let build = session.suspend()?;
                Ok::<_, M11ParserPageError>((builder, build))
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
            .ok_or(M11ParserPageError::InvalidState)?;
        runtime.producer_arena().validate_suspended_build(build)?;
        let build = self.build.take().ok_or(M11ParserPageError::InvalidState)?;
        let result = (|| {
            let mut session = runtime.producer_arena_mut().resume_build(build)?;
            let leaf = session.allocate(&self.page[..payload_len], &[])?;
            self.builder
                .as_mut()
                .ok_or(M11ParserPageError::InvalidState)?
                .begin_push(&session, leaf, &mut self.mutation)?;
            Ok::<_, M11ParserPageError>(session.suspend()?)
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

    fn poll_push(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11ParserPageError> {
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

    fn begin_finish(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11ParserPageError> {
        self.with_resumed_build(runtime, |builder, session, mutation| {
            builder.begin_finish(session, mutation)
        })?;
        self.phase = BuildPhase::Finishing;
        Ok(())
    }

    fn poll_finish(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11ParserPageError> {
        let progress = self.with_resumed_build(runtime, |builder, session, mutation| {
            builder.poll_finish(session, mutation)
        })?;
        self.phase = match progress {
            ResumableSequenceProgress::Pending => BuildPhase::Finishing,
            ResumableSequenceProgress::Complete => BuildPhase::ReadyForRoot,
        };
        Ok(())
    }

    fn take_build_root(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11ParserPageError> {
        self.builder
            .as_ref()
            .ok_or(M11ParserPageError::InvalidState)?;
        let build = self
            .build
            .as_ref()
            .ok_or(M11ParserPageError::InvalidState)?;
        runtime.producer_arena().validate_suspended_build(build)?;
        let build = self.build.take().ok_or(M11ParserPageError::InvalidState)?;
        let session = runtime.producer_arena_mut().resume_build(build)?;
        match self
            .builder
            .as_mut()
            .ok_or(M11ParserPageError::InvalidState)?
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

    fn begin_seal(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11ParserPageError> {
        let build = self
            .build
            .as_ref()
            .ok_or(M11ParserPageError::InvalidState)?;
        runtime.producer_arena().validate_suspended_build(build)?;
        self.build_root
            .as_ref()
            .ok_or(M11ParserPageError::InvalidState)?;
        let build = self.build.take().ok_or(M11ParserPageError::InvalidState)?;
        let root = self
            .build_root
            .take()
            .ok_or(M11ParserPageError::InvalidState)?;
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

    fn poll_seal(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11ParserPageError> {
        let poll = self
            .seal
            .as_mut()
            .ok_or(M11ParserPageError::InvalidState)?
            .poll(runtime.producer_arena_mut(), 1)?;
        let Some(seal_transitions) = self.seal_transitions.checked_add(poll.transitions) else {
            if let Some(tree) = poll.root {
                self.seal = None;
                return self.reject_committed_tree(
                    runtime,
                    tree,
                    M11ParserPageError::CounterOverflow,
                );
            }
            return Err(M11ParserPageError::CounterOverflow);
        };
        self.seal_transitions = seal_transitions;
        let Some(tree) = poll.root else {
            return Ok(());
        };
        self.seal = None;
        self.complete_nonempty(runtime, tree)
    }

    fn complete_nonempty(
        &mut self,
        runtime: &mut DocumentRuntime,
        tree: ParserPageTree,
    ) -> Result<(), M11ParserPageError> {
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
                    M11ParserPageError::Corrupt("nonempty parser page build sealed an empty root"),
                );
            }
            Err(error) => return self.reject_committed_tree(runtime, tree, error),
        };
        if let Err(error) = add_inspection(&mut self.mutation.inspection, inspection) {
            return self.reject_committed_tree(runtime, tree, error);
        }
        let summary = measure.summary();
        let expected_leaves = match u64::try_from(self.mutation.leaves_adopted) {
            Ok(leaves) => leaves,
            Err(_) => {
                return self.reject_committed_tree(
                    runtime,
                    tree,
                    M11ParserPageError::CounterOverflow,
                );
            }
        };
        if summary.stream_tag != self.stream_tag
            || summary.records != self.records
            || summary.payload_bytes != self.payload_bytes
            || summary.encoded_bytes != self.expected_encoded_bytes
            || summary.commitment != self.expected_commitment
            || measure.leaves() != expected_leaves
        {
            return self.reject_committed_tree(
                runtime,
                tree,
                M11ParserPageError::Corrupt(
                    "sealed parser page summary differs from accepted input",
                ),
            );
        }
        if self.lease.is_none() {
            return self.reject_committed_tree(runtime, tree, M11ParserPageError::InvalidState);
        }
        let lease = self
            .lease
            .take()
            .expect("validated parser page source lease");
        self.output = Some(M11ParserPageRoot {
            runtime_identity: self.runtime_identity,
            lease: Some(lease),
            source: self.source,
            source_range: self.source_range.clone(),
            summary,
            page_count: measure.leaves(),
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
        tree: ParserPageTree,
        error: M11ParserPageError,
    ) -> Result<(), M11ParserPageError> {
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
            &mut ParserPageBuilder,
            &mut crate::storage::ArenaBuildSession<'_>,
            &mut SequenceMutationReceipt,
        ) -> Result<T, M11ParserPageError>,
    ) -> Result<T, M11ParserPageError> {
        self.builder
            .as_ref()
            .ok_or(M11ParserPageError::InvalidState)?;
        let build = self
            .build
            .as_ref()
            .ok_or(M11ParserPageError::InvalidState)?;
        runtime.producer_arena().validate_suspended_build(build)?;
        let build = self.build.take().ok_or(M11ParserPageError::InvalidState)?;
        let mut session = runtime.producer_arena_mut().resume_build(build)?;
        let result = operation(
            self.builder
                .as_mut()
                .ok_or(M11ParserPageError::InvalidState)?,
            &mut session,
            &mut self.mutation,
        );
        match result {
            Ok(value) => match session.suspend() {
                Ok(build) => {
                    self.build = Some(build);
                    Ok(value)
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

    fn reset_page(&mut self) {
        self.page.fill(0);
        self.page_len = PAGE_LEAF_HEADER_BYTES;
        self.page_records = 0;
        self.page_payload_bytes = 0;
    }

    fn ensure_runtime(&self, runtime: &DocumentRuntime) -> Result<(), M11ParserPageError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ParserPageError::WrongRuntime);
        }
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11ParserPageError::SourceAuthorityMismatch);
        }
        Ok(())
    }

    fn ensure_runtime_identity(&self, runtime: &DocumentRuntime) -> Result<(), M11ParserPageError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ParserPageError::WrongRuntime);
        }
        Ok(())
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ParserPageError> {
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
        self.pending_record = None;
        self.lease.take();
        self.phase = BuildPhase::Cancelled;
        Ok(())
    }

    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ParserPageReclaimPoll, M11ParserPageError> {
        self.ensure_runtime_identity(runtime)?;
        if self.phase != BuildPhase::Cancelled {
            return Err(M11ParserPageError::InvalidState);
        }
        poll_reclaim(runtime, fuel)
    }

    #[must_use]
    pub fn take_root(&mut self) -> Option<M11ParserPageRoot> {
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
    pub fn receipt(&self) -> M11ParserPageBuildReceipt {
        M11ParserPageBuildReceipt::from_mutation(
            self.transitions,
            self.records,
            self.payload_bytes,
            self.maximum_record_copy_bytes,
            self.mutation,
            self.seal_transitions,
        )
    }
}

impl Drop for M11ParserPageBuild {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.build.is_none()
                    && self.seal.is_none()
                    && self.failed_tree.is_none()
                    && self.output.is_none()
                    && self.lease.is_none(),
                "parser page builds require root transfer or explicit cancellation"
            );
        }
    }
}

/// Immutable exact-source-bound generic parser page root.
#[must_use = "parser page roots require transfer or explicit fuelled release"]
pub struct M11ParserPageRoot {
    runtime_identity: RuntimeIdentity,
    lease: Option<SourceSnapshotLease>,
    source: SourceVersion,
    source_range: Range<usize>,
    summary: PageSummary,
    page_count: u64,
    tree: Option<ParserPageTree>,
    receipt: M11ParserPageBuildReceipt,
    released: bool,
}

impl fmt::Debug for M11ParserPageRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ParserPageRoot")
            .field("source", &self.source)
            .field("source_range", &self.source_range)
            .field("stream_tag", &self.summary.stream_tag)
            .field("page_count", &self.page_count)
            .field("record_count", &self.summary.records)
            .field("payload_bytes", &self.summary.payload_bytes)
            .finish_non_exhaustive()
    }
}

impl M11ParserPageRoot {
    fn empty(
        runtime_identity: RuntimeIdentity,
        lease: SourceSnapshotLease,
        source_range: Range<usize>,
        stream_tag: u32,
        receipt: M11ParserPageBuildReceipt,
    ) -> Self {
        let source = lease.version();
        Self {
            runtime_identity,
            lease: Some(lease),
            source,
            source_range,
            summary: PageSummary::empty(stream_tag),
            page_count: 0,
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
    pub fn source_range(&self) -> Range<usize> {
        self.source_range.clone()
    }

    #[must_use]
    pub const fn stream_tag(&self) -> u32 {
        self.summary.stream_tag
    }

    #[must_use]
    pub const fn page_count(&self) -> u64 {
        self.page_count
    }

    #[must_use]
    pub const fn record_count(&self) -> u64 {
        self.summary.records
    }

    #[must_use]
    pub const fn payload_bytes(&self) -> u64 {
        self.summary.payload_bytes
    }

    #[must_use]
    pub const fn encoded_bytes(&self) -> u64 {
        self.summary.encoded_bytes
    }

    /// Returns a non-cryptographic checksum of the framed record bytes.
    ///
    /// This is an integrity/debugging signal, not a source-authority digest.
    #[must_use]
    pub fn checksum(&self) -> [u8; 32] {
        self.summary.commitment.checksum()
    }

    #[must_use]
    pub const fn build_receipt(&self) -> M11ParserPageBuildReceipt {
        self.receipt
    }

    pub(crate) fn transport_root_id(&self) -> Result<Option<ArenaId>, M11ParserPageError> {
        if self.released || self.lease.is_none() {
            return Err(M11ParserPageError::InvalidState);
        }
        Ok(self.tree.as_ref().and_then(|tree| tree.as_ref().root_id()))
    }

    #[cfg(test)]
    pub(crate) fn tree_root_id_for_test(&self) -> Option<ArenaId> {
        self.transport_root_id().ok().flatten()
    }

    pub fn source_cursor(&self) -> Result<M11ParserRangeCursor, M11ParserPageError> {
        let lease = self
            .lease
            .as_ref()
            .ok_or(M11ParserPageError::InvalidState)?
            .duplicate();
        M11ParserRangeCursor::new(lease, self.source_range.clone())
    }

    pub fn cursor<'root>(
        &'root self,
        runtime: &DocumentRuntime,
    ) -> Result<M11ParserPageCursor<'root>, M11ParserPageError> {
        self.ensure_runtime(runtime)?;
        if self.released {
            return Err(M11ParserPageError::InvalidState);
        }
        Ok(M11ParserPageCursor {
            runtime_identity: self.runtime_identity,
            tree: self
                .tree
                .as_ref()
                .map(CommittedMeasuredSequenceRoot::as_ref),
            page_count: self.page_count,
            record_count: self.summary.records,
            state: M11ParserPageCursorState::default(),
        })
    }

    /// Retains this exact committed measured root into an active publication
    /// journal after revalidating its runtime and full generic summary.
    ///
    /// The caller still owns this root and its source lease. A successful
    /// retain therefore pairs with the caller's ordinary explicit root
    /// release once the typed publication owner has been established.
    pub(crate) fn retain_for_publication(
        &self,
        session: &mut ArenaBuildSession<'_>,
        expected_runtime_identity: RuntimeIdentity,
        expected_source: SourceVersion,
        expected_source_range: Range<usize>,
        expected_stream_tag: u32,
    ) -> Result<M11RetainedParserPageRoot, M11ParserPageError> {
        if self.released
            || self.lease.is_none()
            || self.runtime_identity != expected_runtime_identity
            || self.source != expected_source
            || self.source_range != expected_source_range
            || self.summary.stream_tag != expected_stream_tag
        {
            return Err(M11ParserPageError::SourceAuthorityMismatch);
        }
        let mut receipt = SequenceMutationReceipt::default();
        let owner = match self.tree.as_ref() {
            Some(tree) => {
                let (retained, measure) = retain_committed_measured_sequence_root_with_measure(
                    session,
                    tree,
                    &mut receipt,
                )?;
                if measure.leaves() != self.page_count || measure.summary() != self.summary {
                    return Err(M11ParserPageError::Corrupt(
                        "retained parser page summary changed",
                    ));
                }
                Some(retained.into_owner())
            }
            None => {
                if self.page_count != 0 || self.summary != PageSummary::empty(expected_stream_tag) {
                    return Err(M11ParserPageError::Corrupt(
                        "empty parser page root changed shape",
                    ));
                }
                None
            }
        };
        Ok(M11RetainedParserPageRoot { owner })
    }

    /// Transfers this root into a lifetime-free replay cursor.
    ///
    /// The drain can live beside other owned job state across polls. Returning
    /// it through [`M11ParserPageDrain::into_root`] preserves the root's
    /// existing explicit release authority on both completion and
    /// cancellation.
    #[must_use = "owned parser page drains require root handback and explicit release"]
    pub fn into_drain(self) -> M11ParserPageDrain {
        M11ParserPageDrain {
            state: M11ParserPageCursorState::default(),
            root: self,
        }
    }

    fn ensure_runtime(&self, runtime: &DocumentRuntime) -> Result<(), M11ParserPageError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ParserPageError::WrongRuntime);
        }
        Ok(())
    }

    pub fn begin_release(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ParserPageError> {
        self.ensure_runtime(runtime)?;
        if self.released {
            return Err(M11ParserPageError::InvalidState);
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
    ) -> Result<M11ParserPageReclaimPoll, M11ParserPageError> {
        self.ensure_runtime(runtime)?;
        if !self.released {
            return Err(M11ParserPageError::InvalidState);
        }
        poll_reclaim(runtime, fuel)
    }
}

impl Drop for M11ParserPageRoot {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.released,
                "parser page roots require explicit transfer or fuelled release"
            );
        }
    }
}

pub(crate) fn is_m11_parser_page_node_payload(payload: &[u8]) -> bool {
    matches!(
        payload.get(..4),
        Some(magic) if magic == PAGE_LEAF_MAGIC || magic == PAGE_BRANCH_MAGIC
    )
}

/// Returns the number of canonical parser records packed into one leaf.
///
/// Branches and malformed payloads contribute no record count. Host admission
/// separately validates every matching node before accepting its frame.
pub(crate) fn m11_parser_page_canonical_record_count(payload: &[u8]) -> u32 {
    if payload.get(..4) != Some(PAGE_LEAF_MAGIC.as_slice()) {
        return 0;
    }
    let mut inspection = SequenceSpecInspection::default();
    decode_leaf(payload, &mut inspection)
        .ok()
        .flatten()
        .map_or(0, |leaf| u32::from(leaf.records))
}

/// Validates one postorder-imported generic parser-page node.
///
/// Calling this for every matching snapshot node proves the measured closure
/// incrementally, including each branch's exact relationship to its already
/// admitted children.
pub(crate) fn validate_imported_m11_parser_page_node(
    arena: &PageArena,
    id: ArenaId,
) -> Result<(), M11ParserPageError> {
    if !is_m11_parser_page_node_payload(arena.payload(id)?) {
        return Err(M11ParserPageError::Corrupt(
            "imported parser page node has the wrong payload kind",
        ));
    }
    let mut inspection = SequenceInspectionReceipt::default();
    let _ = validate_measured_sequence_node::<ParserPageSpec>(arena, id, &mut inspection)?;
    Ok(())
}

fn imported_parser_page_root(
    arena: &PageArena,
    root: Option<ArenaId>,
    claim: M11ImportedParserPageRootClaim,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<Option<MeasuredSequenceRef<'static, ParserPageSpec>>, M11ParserPageError> {
    if claim.stream_tag == 0 {
        return Err(M11ParserPageError::InvalidStreamTag);
    }
    let expected_empty = claim.storage_page_count == 0
        && claim.record_count == 0
        && claim.payload_bytes == 0
        && claim.encoded_bytes == 0
        && claim.checksum == [0; 32];
    let Some(root) = root else {
        return if expected_empty {
            Ok(None)
        } else {
            Err(M11ParserPageError::Corrupt(
                "nonempty imported parser stream lost its root",
            ))
        };
    };
    if expected_empty {
        return Err(M11ParserPageError::Corrupt(
            "empty imported parser stream owns a root",
        ));
    }
    let measure = validate_measured_sequence_node::<ParserPageSpec>(arena, root, inspection)?;
    let summary = measure.summary();
    if measure.leaves() != claim.storage_page_count
        || summary.stream_tag != claim.stream_tag
        || summary.records != claim.record_count
        || summary.payload_bytes != claim.payload_bytes
        || summary.encoded_bytes != claim.encoded_bytes
        || summary.commitment.checksum() != claim.checksum
    {
        return Err(M11ParserPageError::Corrupt(
            "imported parser page root differs from its descriptor",
        ));
    }
    Ok(Some(MeasuredSequenceRef::from_imported_root(Some(root))))
}

pub(crate) fn validate_imported_m11_parser_page_root(
    arena: &PageArena,
    root: Option<ArenaId>,
    claim: M11ImportedParserPageRootClaim,
) -> Result<(), M11ParserPageError> {
    let mut inspection = SequenceInspectionReceipt::default();
    let _ = imported_parser_page_root(arena, root, claim, &mut inspection)?;
    Ok(())
}

/// Copies one bounded canonical record from an imported measured root.
pub(crate) fn imported_m11_parser_page_record_at(
    arena: &PageArena,
    root: Option<ArenaId>,
    claim: M11ImportedParserPageRootClaim,
    ordinal: u64,
) -> Result<M11ParserPageRecord, M11ParserPageError> {
    let mut inspection = SequenceInspectionReceipt::default();
    imported_m11_parser_page_record_at_inspected(arena, root, claim, ordinal, &mut inspection)
}

pub(crate) fn imported_m11_parser_page_record_at_inspected(
    arena: &PageArena,
    root: Option<ArenaId>,
    claim: M11ImportedParserPageRootClaim,
    ordinal: u64,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<M11ParserPageRecord, M11ParserPageError> {
    if ordinal >= claim.record_count {
        return Err(M11ParserPageError::InvalidRange);
    }
    let tree = imported_parser_page_root(arena, root, claim, inspection)?.ok_or(
        M11ParserPageError::Corrupt("nonempty imported parser stream lost its root"),
    )?;
    let located = tree
        .locate_leaf_containing_metric(arena, ordinal, |summary| summary.records, inspection)?
        .ok_or(M11ParserPageError::Corrupt(
            "imported parser record ordinal is absent",
        ))?;
    let prefix_records = located.prefix.map_or(0, |summary| summary.records);
    let local_ordinal = usize::try_from(
        ordinal
            .checked_sub(prefix_records)
            .ok_or(M11ParserPageError::CounterOverflow)?,
    )
    .map_err(|_| M11ParserPageError::CounterOverflow)?;
    let payload = arena.payload(located.id)?;
    let leaf = decode_leaf(payload, &mut inspection.spec)?.ok_or(M11ParserPageError::Corrupt(
        "imported parser record routed to a branch",
    ))?;
    if leaf.summary != located.summary || local_ordinal >= usize::from(leaf.records) {
        return Err(M11ParserPageError::Corrupt(
            "imported parser record routing changed",
        ));
    }
    let mut offset = PAGE_LEAF_HEADER_BYTES;
    for index in 0..usize::from(leaf.records) {
        let length_end = offset
            .checked_add(2)
            .ok_or(M11ParserPageError::CounterOverflow)?;
        let len = usize::from(u16::from_le_bytes(
            payload
                .get(offset..length_end)
                .ok_or(M11ParserPageError::Corrupt(
                    "imported parser record length is truncated",
                ))?
                .try_into()
                .expect("checked record length"),
        ));
        offset = length_end;
        let end = offset
            .checked_add(len)
            .ok_or(M11ParserPageError::CounterOverflow)?;
        let bytes = payload.get(offset..end).ok_or(M11ParserPageError::Corrupt(
            "imported parser record payload is truncated",
        ))?;
        if index == local_ordinal {
            return M11ParserPageRecord::new(bytes);
        }
        offset = end;
    }
    Err(M11ParserPageError::Corrupt(
        "imported parser record ordinal is absent from its leaf",
    ))
}

/// Exact source-order replay work for one generic page root.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11ParserPageCursorReceipt {
    transitions: usize,
    pages_entered: u64,
    records_emitted: u64,
    record_bytes_copied: u64,
    maximum_record_copy_bytes: usize,
    node_headers_decoded: u64,
    payload_bytes_inspected: u64,
    items_hashed: u64,
}

impl M11ParserPageCursorReceipt {
    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }

    #[must_use]
    pub const fn pages_entered(self) -> u64 {
        self.pages_entered
    }

    #[must_use]
    pub const fn records_emitted(self) -> u64 {
        self.records_emitted
    }

    #[must_use]
    pub const fn record_bytes_copied(self) -> u64 {
        self.record_bytes_copied
    }

    #[must_use]
    pub const fn maximum_record_copy_bytes(self) -> usize {
        self.maximum_record_copy_bytes
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
    pub const fn items_hashed(self) -> u64 {
        self.items_hashed
    }
}

// Keeping the fixed-capacity record inline avoids a heap allocation on every
// cursor poll. The larger result value is a deliberate bounded-work tradeoff.
#[allow(clippy::large_enum_variant)]
pub enum M11ParserPageCursorPoll {
    Pending {
        transitions: usize,
    },
    Record {
        transitions: usize,
        record: M11ParserPageRecord,
    },
    Complete {
        transitions: usize,
    },
}

impl fmt::Debug for M11ParserPageCursorPoll {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending { transitions } => formatter
                .debug_struct("Pending")
                .field("transitions", transitions)
                .finish(),
            Self::Record {
                transitions,
                record,
            } => formatter
                .debug_struct("Record")
                .field("transitions", transitions)
                .field("record", record)
                .finish(),
            Self::Complete { transitions } => formatter
                .debug_struct("Complete")
                .field("transitions", transitions)
                .finish(),
        }
    }
}

/// Fixed-state source-order cursor over one immutable parser page root.
pub struct M11ParserPageCursor<'root> {
    runtime_identity: RuntimeIdentity,
    tree: Option<MeasuredSequenceRef<'root, ParserPageSpec>>,
    page_count: u64,
    record_count: u64,
    state: M11ParserPageCursorState,
}

#[derive(Default)]
struct M11ParserPageCursorState {
    next_page: u64,
    page_id: Option<ArenaId>,
    page_offset: usize,
    page_end: usize,
    records_remaining: u16,
    records_emitted: u64,
    receipt: M11ParserPageCursorReceipt,
}

impl M11ParserPageCursor<'_> {
    pub fn poll(
        &mut self,
        runtime: &DocumentRuntime,
    ) -> Result<M11ParserPageCursorPoll, M11ParserPageError> {
        poll_page_cursor(
            self.runtime_identity,
            self.tree,
            self.page_count,
            self.record_count,
            &mut self.state,
            runtime,
        )
    }

    #[must_use]
    pub const fn receipt(&self) -> M11ParserPageCursorReceipt {
        self.state.receipt
    }
}

/// Lifetime-free source-order replay owner for one immutable parser page root.
///
/// Draining never discards root authority. Call [`Self::into_root`] after
/// completion, or at any earlier cancellation point, and use the root's
/// existing fuelled release lifecycle.
#[must_use = "owned parser page drains require root handback and explicit release"]
pub struct M11ParserPageDrain {
    root: M11ParserPageRoot,
    state: M11ParserPageCursorState,
}

impl M11ParserPageDrain {
    pub fn poll(
        &mut self,
        runtime: &DocumentRuntime,
    ) -> Result<M11ParserPageCursorPoll, M11ParserPageError> {
        self.root.ensure_runtime(runtime)?;
        if self.root.released {
            return Err(M11ParserPageError::InvalidState);
        }
        let tree = self
            .root
            .tree
            .as_ref()
            .map(CommittedMeasuredSequenceRoot::as_ref);
        poll_page_cursor(
            self.root.runtime_identity,
            tree,
            self.root.page_count,
            self.root.summary.records,
            &mut self.state,
            runtime,
        )
    }

    /// Returns root ownership without requiring replay to complete.
    ///
    /// This is the common handback for successful consumption and
    /// cancellation; the caller then owns explicit root release.
    #[must_use = "parser page roots require transfer or explicit fuelled release"]
    pub fn into_root(self) -> M11ParserPageRoot {
        self.root
    }

    #[must_use]
    pub const fn receipt(&self) -> M11ParserPageCursorReceipt {
        self.state.receipt
    }
}

fn poll_page_cursor(
    runtime_identity: RuntimeIdentity,
    tree: Option<MeasuredSequenceRef<'_, ParserPageSpec>>,
    page_count: u64,
    record_count: u64,
    state: &mut M11ParserPageCursorState,
    runtime: &DocumentRuntime,
) -> Result<M11ParserPageCursorPoll, M11ParserPageError> {
    if runtime.producer_identity() != runtime_identity {
        return Err(M11ParserPageError::WrongRuntime);
    }
    if state.records_remaining > 0 {
        let id = state.page_id.ok_or(M11ParserPageError::InvalidState)?;
        let payload = runtime.producer_arena().payload(id)?;
        if state.page_offset + 2 > state.page_end {
            return Err(M11ParserPageError::Corrupt(
                "parser page record length exceeds its leaf",
            ));
        }
        let len = usize::from(u16::from_le_bytes(
            payload[state.page_offset..state.page_offset + 2]
                .try_into()
                .expect("checked record length"),
        ));
        state.page_offset += 2;
        if len == 0
            || len > M11_PARSER_PAGE_MAX_RECORD_BYTES
            || state.page_offset + len > state.page_end
        {
            return Err(M11ParserPageError::Corrupt(
                "parser page record payload is invalid",
            ));
        }
        let record =
            M11ParserPageRecord::new(&payload[state.page_offset..state.page_offset + len])?;
        state.page_offset += len;
        state.records_remaining -= 1;
        let records_emitted = state
            .records_emitted
            .checked_add(1)
            .ok_or(M11ParserPageError::CounterOverflow)?;
        if records_emitted > record_count {
            return Err(M11ParserPageError::Corrupt(
                "parser page cursor exceeded its root record count",
            ));
        }
        state.records_emitted = records_emitted;
        state.receipt.transitions = state
            .receipt
            .transitions
            .checked_add(1)
            .ok_or(M11ParserPageError::CounterOverflow)?;
        state.receipt.records_emitted = state.records_emitted;
        state.receipt.record_bytes_copied = state
            .receipt
            .record_bytes_copied
            .checked_add(u64::try_from(len).map_err(|_| M11ParserPageError::CounterOverflow)?)
            .ok_or(M11ParserPageError::CounterOverflow)?;
        state.receipt.maximum_record_copy_bytes = state.receipt.maximum_record_copy_bytes.max(len);
        return Ok(M11ParserPageCursorPoll::Record {
            transitions: 1,
            record,
        });
    }
    if state.page_id.is_some() {
        if state.page_offset != state.page_end {
            return Err(M11ParserPageError::Corrupt(
                "parser page leaf has trailing record bytes",
            ));
        }
        state.page_id = None;
    }
    if state.next_page == page_count {
        if state.records_emitted != record_count {
            return Err(M11ParserPageError::Corrupt(
                "parser page cursor record count differs from its root",
            ));
        }
        return Ok(M11ParserPageCursorPoll::Complete { transitions: 0 });
    }
    let tree = tree.ok_or(M11ParserPageError::Corrupt(
        "nonempty parser page cursor lost its root",
    ))?;
    let mut inspection = SequenceInspectionReceipt::default();
    let located = tree
        .locate_leaf_with_prefix(runtime.producer_arena(), state.next_page, &mut inspection)?
        .ok_or(M11ParserPageError::Corrupt(
            "parser page ordinal is absent from its root",
        ))?;
    let payload = runtime.producer_arena().payload(located.id)?;
    let leaf = decode_leaf(payload, &mut inspection.spec)?.ok_or(M11ParserPageError::Corrupt(
        "measured parser page leaf uses a branch payload",
    ))?;
    add_cursor_inspection(&mut state.receipt, inspection)?;
    if located.ordinal != state.next_page || leaf.summary != located.summary {
        return Err(M11ParserPageError::Corrupt(
            "parser page leaf authority changed during replay",
        ));
    }
    state.page_id = Some(located.id);
    state.page_offset = PAGE_LEAF_HEADER_BYTES;
    state.page_end = leaf.end;
    state.records_remaining = leaf.records;
    state.next_page += 1;
    state.receipt.transitions = state
        .receipt
        .transitions
        .checked_add(1)
        .ok_or(M11ParserPageError::CounterOverflow)?;
    state.receipt.pages_entered = state
        .receipt
        .pages_entered
        .checked_add(1)
        .ok_or(M11ParserPageError::CounterOverflow)?;
    Ok(M11ParserPageCursorPoll::Pending { transitions: 1 })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ParserPageReclaimPoll {
    receipt: ReclaimReceipt,
    complete: bool,
}

impl M11ParserPageReclaimPoll {
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
) -> Result<M11ParserPageReclaimPoll, M11ParserPageError> {
    validate_fuel(fuel)?;
    let receipt = runtime.producer_arena_mut().poll_reclaim(fuel);
    let metrics = runtime.arena_metrics();
    Ok(M11ParserPageReclaimPoll {
        receipt,
        complete: metrics.pending_build_aborts == 0 && metrics.pending_reclaims == 0,
    })
}

fn validate_fuel(fuel: usize) -> Result<(), M11ParserPageError> {
    if fuel == 0 {
        return Err(M11ParserPageError::ZeroFuel);
    }
    if fuel > M11_PARSER_PAGE_MAX_POLL_TRANSITIONS {
        return Err(M11ParserPageError::PollLimitExceeded);
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct DecodedLeaf {
    summary: PageSummary,
    records: u16,
    end: usize,
}

fn decode_leaf(
    payload: &[u8],
    inspection: &mut SequenceSpecInspection,
) -> Result<Option<DecodedLeaf>, M11ParserPageError> {
    if payload.get(..4) != Some(PAGE_LEAF_MAGIC.as_slice()) {
        return Ok(None);
    }
    if payload.len() < PAGE_LEAF_HEADER_BYTES {
        return Err(M11ParserPageError::Corrupt(
            "parser page leaf is shorter than its header",
        ));
    }
    inspection
        .charge_payload_bytes(payload.len())
        .ok_or(M11ParserPageError::CounterOverflow)?;
    let mut cursor = 4;
    let schema = read_u32(payload, &mut cursor)?;
    if schema != PAGE_SCHEMA {
        return Err(M11ParserPageError::Corrupt(
            "parser page leaf schema is unsupported",
        ));
    }
    let stream_tag = read_u32(payload, &mut cursor)?;
    let records = read_u16(payload, &mut cursor)?;
    let encoded_bytes = usize::from(read_u16(payload, &mut cursor)?);
    let payload_bytes = usize::from(read_u16(payload, &mut cursor)?);
    let reserved = read_u16(payload, &mut cursor)?;
    if stream_tag == 0
        || records == 0
        || reserved != 0
        || cursor != PAGE_LEAF_HEADER_BYTES
        || PAGE_LEAF_HEADER_BYTES + encoded_bytes != payload.len()
    {
        return Err(M11ParserPageError::Corrupt(
            "parser page leaf metadata is invalid",
        ));
    }
    let encoded = &payload[PAGE_LEAF_HEADER_BYTES..];
    let mut record_cursor = 0;
    let mut observed_payload = 0usize;
    for _ in 0..records {
        if record_cursor + 2 > encoded.len() {
            return Err(M11ParserPageError::Corrupt(
                "parser page leaf record length is truncated",
            ));
        }
        let len = usize::from(u16::from_le_bytes(
            encoded[record_cursor..record_cursor + 2]
                .try_into()
                .expect("checked record length"),
        ));
        record_cursor += 2;
        if len == 0 || len > M11_PARSER_PAGE_MAX_RECORD_BYTES || record_cursor + len > encoded.len()
        {
            return Err(M11ParserPageError::Corrupt(
                "parser page leaf record is invalid",
            ));
        }
        record_cursor += len;
        observed_payload = observed_payload
            .checked_add(len)
            .ok_or(M11ParserPageError::CounterOverflow)?;
    }
    if record_cursor != encoded.len() || observed_payload != payload_bytes {
        return Err(M11ParserPageError::Corrupt(
            "parser page leaf record totals are inconsistent",
        ));
    }
    inspection
        .charge_hashed_items(encoded.len())
        .ok_or(M11ParserPageError::CounterOverflow)?;
    Ok(Some(DecodedLeaf {
        summary: PageSummary {
            stream_tag,
            records: u64::from(records),
            payload_bytes: u64::try_from(payload_bytes)
                .map_err(|_| M11ParserPageError::CounterOverflow)?,
            encoded_bytes: u64::try_from(encoded_bytes)
                .map_err(|_| M11ParserPageError::CounterOverflow)?,
            commitment: PageCommitment::for_bytes(encoded),
        },
        records,
        end: payload.len(),
    }))
}

fn encode_leaf_header(
    page: &mut [u8; ARENA_PAGE_BYTES],
    stream_tag: u32,
    records: u16,
    encoded_bytes: usize,
    payload_bytes: usize,
) -> Result<(), M11ParserPageError> {
    if stream_tag == 0
        || records == 0
        || encoded_bytes == 0
        || PAGE_LEAF_HEADER_BYTES + encoded_bytes > page.len()
    {
        return Err(M11ParserPageError::Corrupt(
            "parser page leaf input is invalid",
        ));
    }
    let mut cursor = 0;
    write_bytes(page, &mut cursor, &PAGE_LEAF_MAGIC)?;
    write_u32(page, &mut cursor, PAGE_SCHEMA)?;
    write_u32(page, &mut cursor, stream_tag)?;
    write_u16(page, &mut cursor, records)?;
    write_u16(
        page,
        &mut cursor,
        u16::try_from(encoded_bytes).map_err(|_| M11ParserPageError::CounterOverflow)?,
    )?;
    write_u16(
        page,
        &mut cursor,
        u16::try_from(payload_bytes).map_err(|_| M11ParserPageError::CounterOverflow)?,
    )?;
    write_u16(page, &mut cursor, 0)?;
    if cursor != PAGE_LEAF_HEADER_BYTES {
        return Err(M11ParserPageError::Corrupt(
            "parser page leaf header length changed",
        ));
    }
    Ok(())
}

fn add_inspection(
    target: &mut SequenceInspectionReceipt,
    delta: SequenceInspectionReceipt,
) -> Result<(), M11ParserPageError> {
    target.node_headers_decoded = target
        .node_headers_decoded
        .checked_add(delta.node_headers_decoded)
        .ok_or(M11ParserPageError::CounterOverflow)?;
    target.summary_combinations = target
        .summary_combinations
        .checked_add(delta.summary_combinations)
        .ok_or(M11ParserPageError::CounterOverflow)?;
    target.spec.payload_bytes_inspected = target
        .spec
        .payload_bytes_inspected
        .checked_add(delta.spec.payload_bytes_inspected)
        .ok_or(M11ParserPageError::CounterOverflow)?;
    target.spec.spec_items_hashed = target
        .spec
        .spec_items_hashed
        .checked_add(delta.spec.spec_items_hashed)
        .ok_or(M11ParserPageError::CounterOverflow)?;
    Ok(())
}

fn add_cursor_inspection(
    target: &mut M11ParserPageCursorReceipt,
    delta: SequenceInspectionReceipt,
) -> Result<(), M11ParserPageError> {
    target.node_headers_decoded = target
        .node_headers_decoded
        .checked_add(delta.node_headers_decoded)
        .ok_or(M11ParserPageError::CounterOverflow)?;
    target.payload_bytes_inspected = target
        .payload_bytes_inspected
        .checked_add(delta.spec.payload_bytes_inspected)
        .ok_or(M11ParserPageError::CounterOverflow)?;
    target.items_hashed = target
        .items_hashed
        .checked_add(delta.spec.spec_items_hashed)
        .ok_or(M11ParserPageError::CounterOverflow)?;
    Ok(())
}

fn encode_commitment(
    commitment: PageCommitment,
    output: &mut [u8],
    cursor: &mut usize,
) -> Result<(), M11ParserPageError> {
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
) -> Result<PageCommitment, M11ParserPageError> {
    let mut hash = [0_u64; COMMITMENT_LANES];
    let mut factor = [0_u64; COMMITMENT_LANES];
    for value in &mut hash {
        *value = read_u64(input, cursor)?;
    }
    for value in &mut factor {
        *value = read_u64(input, cursor)?;
    }
    Ok(PageCommitment { hash, factor })
}

fn write_bytes(
    output: &mut [u8],
    cursor: &mut usize,
    bytes: &[u8],
) -> Result<(), M11ParserPageError> {
    let end = cursor
        .checked_add(bytes.len())
        .ok_or(M11ParserPageError::CounterOverflow)?;
    let target = output
        .get_mut(*cursor..end)
        .ok_or(M11ParserPageError::Corrupt(
            "parser page encoding exceeded fixed scratch",
        ))?;
    target.copy_from_slice(bytes);
    *cursor = end;
    Ok(())
}

fn write_u16(output: &mut [u8], cursor: &mut usize, value: u16) -> Result<(), M11ParserPageError> {
    write_bytes(output, cursor, &value.to_le_bytes())
}

fn write_u32(output: &mut [u8], cursor: &mut usize, value: u32) -> Result<(), M11ParserPageError> {
    write_bytes(output, cursor, &value.to_le_bytes())
}

fn write_u64(output: &mut [u8], cursor: &mut usize, value: u64) -> Result<(), M11ParserPageError> {
    write_bytes(output, cursor, &value.to_le_bytes())
}

fn read_u16(input: &[u8], cursor: &mut usize) -> Result<u16, M11ParserPageError> {
    let end = cursor
        .checked_add(2)
        .ok_or(M11ParserPageError::CounterOverflow)?;
    let bytes = input
        .get(*cursor..end)
        .ok_or(M11ParserPageError::Corrupt("parser page u16 is truncated"))?;
    *cursor = end;
    Ok(u16::from_le_bytes(
        bytes.try_into().expect("checked u16 width"),
    ))
}

fn read_u32(input: &[u8], cursor: &mut usize) -> Result<u32, M11ParserPageError> {
    let end = cursor
        .checked_add(4)
        .ok_or(M11ParserPageError::CounterOverflow)?;
    let bytes = input
        .get(*cursor..end)
        .ok_or(M11ParserPageError::Corrupt("parser page u32 is truncated"))?;
    *cursor = end;
    Ok(u32::from_le_bytes(
        bytes.try_into().expect("checked u32 width"),
    ))
}

fn read_u64(input: &[u8], cursor: &mut usize) -> Result<u64, M11ParserPageError> {
    let end = cursor
        .checked_add(8)
        .ok_or(M11ParserPageError::CounterOverflow)?;
    let bytes = input
        .get(*cursor..end)
        .ok_or(M11ParserPageError::Corrupt("parser page u64 is truncated"))?;
    *cursor = end;
    Ok(u64::from_le_bytes(
        bytes.try_into().expect("checked u64 width"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentRuntimeConfig;

    fn close_runtime(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin close");
        while !runtime.poll_close(64).expect("poll close").complete {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
        assert_eq!(runtime.arena_metrics().live_builds, 0);
    }

    #[test]
    fn committed_summary_rejection_releases_its_tree_fuelfully() {
        let text = "authenticated source";
        let mut runtime =
            DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build = M11ParserPageBuild::new(&runtime, lease, 0..text.len(), 71).expect("build");
        build
            .offer_record(M11ParserPageRecord::new(b"record").expect("record"))
            .expect("offer");
        while build.poll(&mut runtime, 1).expect("record poll").status()
            != M11ParserPageBuildStatus::NeedsInput
        {}
        build.finish_input().expect("finish input");

        // Fault-inject a disagreement between independently accumulated input
        // and the sealed measured tree. The committed root must never be lost.
        build.expected_commitment = PageCommitment::empty();
        loop {
            match build.poll(&mut runtime, 1) {
                Ok(poll) => assert_ne!(poll.status(), M11ParserPageBuildStatus::Complete),
                Err(M11ParserPageError::Corrupt(
                    "sealed parser page summary differs from accepted input",
                )) => break,
                Err(error) => panic!("unexpected build error: {error}"),
            }
        }
        build.begin_cancel(&mut runtime).expect("begin cancel");
        while !build
            .poll_cancel(&mut runtime, 1)
            .expect("poll cancel")
            .complete()
        {}
        drop(build);
        close_runtime(runtime);
    }

    #[test]
    fn cursor_terminal_authenticates_expected_record_count() {
        let runtime = DocumentRuntime::new("", DocumentRuntimeConfig::default()).expect("runtime");
        let mut cursor = M11ParserPageCursor {
            runtime_identity: runtime.producer_identity(),
            tree: None,
            page_count: 0,
            record_count: 1,
            state: M11ParserPageCursorState::default(),
        };
        assert!(matches!(
            cursor.poll(&runtime),
            Err(M11ParserPageError::Corrupt(
                "parser page cursor record count differs from its root"
            ))
        ));
        close_runtime(runtime);
    }
}
