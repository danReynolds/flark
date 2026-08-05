//! One-pass, candidate-bound physical-source ledger.
//!
//! This is the production-shaped Stage 1 source seam. It deliberately reuses
//! [`LiveCandidateEpoch`], [`ArenaBuildId`], the candidate-owned Crop cursor,
//! and the live document's existing block/coverage permits. Parser code sees
//! source atoms and opaque boundaries, never caller-constructed ranges or
//! byte/UTF-16 metrics. Claims are emitted in source order and every retained
//! semantic reference is checked against the candidate's live open path.
//!
//! This slice stops before the packed green sink. The returned
//! [`ValidatedSourceClaim`] values are typed test/debug records; their digest
//! is explicitly not publication or adoption authority.

use std::fmt;

#[cfg(feature = "exact-parser")]
use crate::storage_only_composite_document::{
    ParentSelectedRestartCompositeAdoptionLease, RestartCompositeAdoptionLease,
    RestartCompositeDocumentError, RestartParentSelectionStamp,
};
use crate::{
    ArenaBuildId, BlockId, CoverageId, CoveragePart, CropSourceCursor, CursorMetrics,
    FreshBlockPermit, FreshCoveragePermit, GreenAffinity, GreenComposerTailAdoptionAuthority,
    GreenDeferredLineEnding, GreenKind, LiveCandidateEpoch, SerializedMetric,
    SourceBoundGreenTailAdoption, SourceByte, SourcePhysicalLineDescriptor,
    SourcePhysicalLineEnding, SourceSnapshotDescriptor,
};
#[cfg(feature = "exact-parser")]
use flark_comrak_value_block_core::{
    DirectBlockKind, DirectLineBoundaryDeferredRole, DirectLineBoundaryResumeCursor,
    DirectValueBlockParser, ParseError,
};

/// Schema version for the physical-part/logical-action contract.
pub const SOURCE_BOUND_LEDGER_SCHEMA_VERSION: u16 = 1;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Friend authority proving that immutable relative-depth green coverage is
/// being interpreted against the live candidate path owned by this module.
/// The private field prevents callers from manufacturing the rebind.
pub(crate) struct GreenTailOpenPathRebindMint(());

/// Exact metric derived by the source decoder. Its fields are intentionally
/// private: parser-facing calls can observe a result but cannot supply one.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceLedgerMetric {
    bytes: u64,
    utf16: u64,
}

impl SourceLedgerMetric {
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }

    #[must_use]
    pub const fn utf16(self) -> u64 {
        self.utf16
    }

    fn checked_add(self, other: Self) -> Result<Self, SourceBoundLedgerError> {
        Ok(Self {
            bytes: self
                .bytes
                .checked_add(other.bytes)
                .ok_or(SourceBoundLedgerError::Overflow("source bytes"))?,
            utf16: self
                .utf16
                .checked_add(other.utf16)
                .ok_or(SourceBoundLedgerError::Overflow("source UTF-16"))?,
        })
    }

    fn checked_sub(self, other: Self) -> Result<Self, SourceBoundLedgerError> {
        Ok(Self {
            bytes: self
                .bytes
                .checked_sub(other.bytes)
                .ok_or(SourceBoundLedgerError::Invariant("metric byte order"))?,
            utf16: self
                .utf16
                .checked_sub(other.utf16)
                .ok_or(SourceBoundLedgerError::Invariant("metric UTF-16 order"))?,
        })
    }
}

impl fmt::Debug for SourceLedgerMetric {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}b/{}u16", self.bytes, self.utf16)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SpecialCounts {
    tabs: u64,
    nuls: u64,
    line_endings: u64,
}

impl SpecialCounts {
    fn checked_sub(self, earlier: Self) -> Result<Self, SourceBoundLedgerError> {
        Ok(Self {
            tabs: self
                .tabs
                .checked_sub(earlier.tabs)
                .ok_or(SourceBoundLedgerError::Invariant("tab count order"))?,
            nuls: self
                .nuls
                .checked_sub(earlier.nuls)
                .ok_or(SourceBoundLedgerError::Invariant("NUL count order"))?,
            line_endings: self
                .line_endings
                .checked_sub(earlier.line_endings)
                .ok_or(SourceBoundLedgerError::Invariant("line-ending count order"))?,
        })
    }

    const fn is_zero(self) -> bool {
        self.tabs == 0 && self.nuls == 0 && self.line_endings == 0
    }
}

/// Physical line-ending shape certified from exact source bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateLineEnding {
    Lf,
    LoneCr,
    CrLf,
}

impl CandidateLineEnding {
    const fn bytes(self) -> u64 {
        match self {
            Self::Lf | Self::LoneCr => 1,
            Self::CrLf => 2,
        }
    }
}

/// One complete UTF-8/source atom. No atom can split a scalar or CRLF pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateSourceAtomKind {
    Scalar(char),
    Tab,
    Nul,
    LineEnding(CandidateLineEnding),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BoundaryScope {
    build: ArenaBuildId,
    line_ordinal: u64,
    line_start: u64,
    absolute_end: u64,
    metric_at_end: SourceLedgerMetric,
    special_at_end: SpecialCounts,
}

/// Non-forgeable scalar boundary minted by the candidate-owned decoder.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CandidateSourceBoundary {
    scope: BoundaryScope,
}

impl CandidateSourceBoundary {
    /// Query coordinate only. Supplying the same scalar to another API does
    /// not recreate this capability.
    #[must_use]
    pub const fn absolute_offset(self) -> u64 {
        self.scope.absolute_end
    }
}

impl fmt::Debug for CandidateSourceBoundary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateSourceBoundary")
            .field("build", &self.scope.build)
            .field("line", &self.scope.line_ordinal)
            .field("absolute_end", &self.scope.absolute_end)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CertifiedAtom {
    build: ArenaBuildId,
    line_ordinal: u64,
    line_start: u64,
    absolute_start: u64,
    absolute_end: u64,
    metric_after: SourceLedgerMetric,
    special_after: SpecialCounts,
    kind: CandidateSourceAtomKind,
}

/// Exact source atom returned to parser recognition. Its private certificate,
/// not its debug coordinates, authorizes a typed atomic projection.
#[derive(PartialEq, Eq)]
pub struct CandidateSourceAtom {
    certified: CertifiedAtom,
}

impl CandidateSourceAtom {
    #[must_use]
    pub const fn kind(&self) -> CandidateSourceAtomKind {
        self.certified.kind
    }

    #[must_use]
    pub const fn boundary(&self) -> CandidateSourceBoundary {
        CandidateSourceBoundary {
            scope: BoundaryScope {
                build: self.certified.build,
                line_ordinal: self.certified.line_ordinal,
                line_start: self.certified.line_start,
                absolute_end: self.certified.absolute_end,
                metric_at_end: self.certified.metric_after,
                special_at_end: self.certified.special_after,
            },
        }
    }

    #[must_use]
    pub const fn absolute_range(&self) -> (u64, u64) {
        (self.certified.absolute_start, self.certified.absolute_end)
    }
}

impl fmt::Debug for CandidateSourceAtom {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateSourceAtom")
            .field("kind", &self.certified.kind)
            .field(
                "range",
                &(self.certified.absolute_start..self.certified.absolute_end),
            )
            .finish_non_exhaustive()
    }
}

/// Bounded work receipt for one source poll.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CandidateSourcePollReceipt {
    pub work_units: usize,
    pub source_bytes_read: usize,
}

/// One resumable source-poll outcome.
#[derive(Debug, PartialEq, Eq)]
pub enum CandidateSourcePoll {
    NeedFuel(CandidateSourcePollReceipt),
    Atom {
        atom: CandidateSourceAtom,
        receipt: CandidateSourcePollReceipt,
    },
    Eof(CandidateSourcePollReceipt),
}

/// Read-only atom from the speculative recognition cursor. No claim or
/// projection API accepts this type; only the authoritative cursor mints
/// [`CandidateSourceAtom`] capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateRecognitionAtom {
    kind: CandidateSourceAtomKind,
    absolute_start: u64,
    absolute_end: u64,
}

impl CandidateRecognitionAtom {
    #[must_use]
    pub const fn kind(self) -> CandidateSourceAtomKind {
        self.kind
    }

    #[must_use]
    pub const fn absolute_range(self) -> (u64, u64) {
        (self.absolute_start, self.absolute_end)
    }
}

/// Opaque recognition position. It can key resumable grammar state but cannot
/// be converted into a source boundary or used to claim bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CandidateRecognitionCheckpoint {
    descriptor: SourceSnapshotDescriptor,
    build: ArenaBuildId,
    line_ordinal: u64,
    absolute_offset: u64,
}

impl CandidateRecognitionCheckpoint {
    #[must_use]
    pub const fn source(self) -> SourceSnapshotDescriptor {
        self.descriptor
    }

    #[must_use]
    pub const fn build_id(self) -> ArenaBuildId {
        self.build
    }

    #[must_use]
    pub const fn line_ordinal(self) -> u64 {
        self.line_ordinal
    }

    #[must_use]
    pub const fn absolute_offset(self) -> u64 {
        self.absolute_offset
    }
}

impl fmt::Debug for CandidateRecognitionCheckpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateRecognitionCheckpoint")
            .field("source", &self.descriptor)
            .field("build", &self.build)
            .field("line", &self.line_ordinal)
            .field("offset", &self.absolute_offset)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CandidateRecognitionPoll {
    NeedFuel(CandidateSourcePollReceipt),
    Atom {
        atom: CandidateRecognitionAtom,
        checkpoint: CandidateRecognitionCheckpoint,
        receipt: CandidateSourcePollReceipt,
    },
    Eof(CandidateSourcePollReceipt),
}

/// Hard cap on source accesses made through one borrowed recognition-byte
/// view. New sequential reads and permitted last-byte repeats share this same
/// actor budget.
pub const CANDIDATE_RECOGNITION_BYTE_POLL_MAX_ACCESSES: usize = 4 * 1024;

/// Opaque identity of one candidate-owned, physical-line-bounded byte session.
///
/// The complete live epoch is retained in addition to source/build/line and
/// exact physical bounds. Scanner code can inspect coordinates but cannot
/// construct a session or turn it into source-claim authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateRecognitionByteSession {
    epoch: LiveCandidateEpoch,
    source: SourceSnapshotDescriptor,
    line_ordinal: u64,
    start: usize,
    content_end: usize,
    end: usize,
    content_utf16: usize,
    physical_utf16: usize,
    ending: SourcePhysicalLineEnding,
}

impl CandidateRecognitionByteSession {
    #[must_use]
    pub const fn epoch(self) -> LiveCandidateEpoch {
        self.epoch
    }

    #[must_use]
    pub const fn source(self) -> SourceSnapshotDescriptor {
        self.source
    }

    #[must_use]
    pub const fn build_id(self) -> ArenaBuildId {
        self.epoch.build_id()
    }

    #[must_use]
    pub const fn line_ordinal(self) -> u64 {
        self.line_ordinal
    }

    #[must_use]
    pub const fn start(self) -> usize {
        self.start
    }

    #[must_use]
    pub const fn content_end(self) -> usize {
        self.content_end
    }

    #[must_use]
    pub const fn end(self) -> usize {
        self.end
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    #[must_use]
    pub const fn content_utf16(self) -> usize {
        self.content_utf16
    }

    #[must_use]
    pub const fn physical_utf16(self) -> usize {
        self.physical_utf16
    }

    #[must_use]
    pub const fn ending(self) -> SourcePhysicalLineEnding {
        self.ending
    }
}

/// A scanner-visible access failure. Budget exhaustion is resumable; every
/// other variant permanently fails the active unpublished session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateRecognitionByteAccessError {
    BudgetExhausted,
    LogicalEof {
        requested: usize,
        len: usize,
    },
    OutOfOrder {
        requested: usize,
        next_sequential: usize,
    },
    SessionFailed,
    Infrastructure(SourceBoundLedgerError),
}

/// Grammar-local scanner state receives this borrowed source only for one
/// actor poll. It cannot retain the candidate, cursor, or Crop root.
pub trait CandidateRecognitionByteScanner {
    type Error;

    fn poll(&mut self, source: &mut CandidateRecognitionByteSource<'_>) -> Result<(), Self::Error>;
}

impl<F, E> CandidateRecognitionByteScanner for F
where
    F: FnMut(&mut CandidateRecognitionByteSource<'_>) -> Result<(), E>,
{
    type Error = E;

    fn poll(&mut self, source: &mut CandidateRecognitionByteSource<'_>) -> Result<(), Self::Error> {
        self(source)
    }
}

/// Keeps candidate/source failures distinct from grammar-local scanner
/// failures. Either non-budget failure leaves an open session that can only be
/// abandoned by cancelling the whole candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateRecognitionBytePollError<Infrastructure, Scanner> {
    Infrastructure(Infrastructure),
    Scanner(Scanner),
}

/// Bounded diagnostic receipt for one borrowed byte-source poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateRecognitionBytePollReceipt {
    session: CandidateRecognitionByteSession,
    start_exposed_high_water: usize,
    end_exposed_high_water: usize,
    physical_high_water: u64,
    access_work_units: usize,
    new_bytes: usize,
    source_bytes_read: usize,
    repeated_last_byte_peeks: usize,
    decoded_atoms: usize,
    budget_exhausted: bool,
    maximum_retained_byte_scratch: usize,
}

impl CandidateRecognitionBytePollReceipt {
    #[must_use]
    pub const fn session(self) -> CandidateRecognitionByteSession {
        self.session
    }

    #[must_use]
    pub const fn exposed_high_water(self) -> (usize, usize) {
        (self.start_exposed_high_water, self.end_exposed_high_water)
    }

    /// Absolute decoder read high-water. It is deliberately distinct from
    /// the scanner-owned logical cursor and the source view's exposed offset.
    #[must_use]
    pub const fn physical_high_water(self) -> u64 {
        self.physical_high_water
    }

    #[must_use]
    pub const fn access_work_units(self) -> usize {
        self.access_work_units
    }

    #[must_use]
    pub const fn new_bytes(self) -> usize {
        self.new_bytes
    }

    #[must_use]
    pub const fn source_bytes_read(self) -> usize {
        self.source_bytes_read
    }

    #[must_use]
    pub const fn repeated_last_byte_peeks(self) -> usize {
        self.repeated_last_byte_peeks
    }

    #[must_use]
    pub const fn decoded_atoms(self) -> usize {
        self.decoded_atoms
    }

    #[must_use]
    pub const fn budget_exhausted(self) -> bool {
        self.budget_exhausted
    }

    #[must_use]
    pub const fn maximum_retained_byte_scratch(self) -> usize {
        self.maximum_retained_byte_scratch
    }
}

/// Completion receipt for one exact physical line. The embedded recognition
/// receipt installs the same authoritative replay expectation as scalar
/// recognition; byte-session diagnostics remain non-authoritative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateRecognitionByteSessionFinishReceipt {
    line: CandidateRecognitionLineReceipt,
    session: CandidateRecognitionByteSession,
    total_access_work_units: u64,
    new_bytes: u64,
    source_bytes_read: u64,
    repeated_last_byte_peeks: u64,
    decoded_atoms: u64,
    physical_high_water: u64,
    maximum_retained_byte_scratch: usize,
}

impl CandidateRecognitionByteSessionFinishReceipt {
    #[must_use]
    pub const fn line(self) -> CandidateRecognitionLineReceipt {
        self.line
    }

    #[must_use]
    pub const fn session(self) -> CandidateRecognitionByteSession {
        self.session
    }

    #[must_use]
    pub const fn total_access_work_units(self) -> u64 {
        self.total_access_work_units
    }

    #[must_use]
    pub const fn new_bytes(self) -> u64 {
        self.new_bytes
    }

    #[must_use]
    pub const fn source_bytes_read(self) -> u64 {
        self.source_bytes_read
    }

    #[must_use]
    pub const fn repeated_last_byte_peeks(self) -> u64 {
        self.repeated_last_byte_peeks
    }

    #[must_use]
    pub const fn decoded_atoms(self) -> u64 {
        self.decoded_atoms
    }

    #[must_use]
    pub const fn physical_high_water(self) -> u64 {
        self.physical_high_water
    }

    #[must_use]
    pub const fn maximum_retained_byte_scratch(self) -> usize {
        self.maximum_retained_byte_scratch
    }
}

/// Hard scheduler bound for one recognition-window transition. A caller may
/// grant more fuel, but the ledger never spends more than this many shared
/// decoder work units before returning control to the actor.
pub const CANDIDATE_RECOGNITION_WINDOW_MAX_WORK: usize = 4 * 1024;

/// A read-only consumer of speculative recognition atoms.
///
/// The sink receives only [`CandidateRecognitionAtom`], which deliberately has
/// no source-boundary certificate. Implementations may update parser-local
/// scanner state, but cannot feed anything received here into source-claim or
/// writer-consumption APIs.
pub trait CandidateRecognitionSink {
    type Error;

    fn push_recognition_atom(&mut self, atom: CandidateRecognitionAtom) -> Result<(), Self::Error>;
}

impl<F, E> CandidateRecognitionSink for F
where
    F: FnMut(CandidateRecognitionAtom) -> Result<(), E>,
{
    type Error = E;

    fn push_recognition_atom(&mut self, atom: CandidateRecognitionAtom) -> Result<(), Self::Error> {
        self(atom)
    }
}

/// Why one bounded recognition window yielded to its actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateRecognitionWindowStatus {
    BudgetExhausted,
    LineEnded(CandidateLineEnding),
    Eof,
}

/// One bounded recognition-window receipt. Both checkpoints are query-only
/// coordinates; neither can authorize source consumption.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateRecognitionWindowReceipt {
    start: CandidateRecognitionCheckpoint,
    end: CandidateRecognitionCheckpoint,
    work: CandidateSourcePollReceipt,
    atoms_emitted: usize,
    status: CandidateRecognitionWindowStatus,
}

impl CandidateRecognitionWindowReceipt {
    #[must_use]
    pub const fn start(self) -> CandidateRecognitionCheckpoint {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> CandidateRecognitionCheckpoint {
        self.end
    }

    #[must_use]
    pub const fn work(self) -> CandidateSourcePollReceipt {
        self.work
    }

    #[must_use]
    pub const fn atoms_emitted(self) -> usize {
        self.atoms_emitted
    }

    #[must_use]
    pub const fn status(self) -> CandidateRecognitionWindowStatus {
        self.status
    }
}

/// Keeps source/writer failures distinct from parser-local sink failures. A
/// sink failure may occur after the recognition cursor advanced; the writer
/// facade therefore poisons the unpublished candidate before returning it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateRecognitionWindowError<Infrastructure, Sink> {
    Infrastructure(Infrastructure),
    Sink(Sink),
}

/// Debug/query receipt for one fully recognized physical line. Finishing it
/// installs one O(1) replay expectation inside the candidate; the receipt
/// itself is not accepted as source or claim authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateRecognitionLineReceipt {
    descriptor: SourceSnapshotDescriptor,
    build: ArenaBuildId,
    line_ordinal: u64,
    absolute_start: u64,
    absolute_end: u64,
    metric: SourceLedgerMetric,
    ending: Option<CandidateLineEnding>,
    atom_count: u64,
    atom_debug_digest: u64,
}

impl CandidateRecognitionLineReceipt {
    #[must_use]
    pub const fn source(self) -> SourceSnapshotDescriptor {
        self.descriptor
    }

    #[must_use]
    pub const fn build_id(self) -> ArenaBuildId {
        self.build
    }

    #[must_use]
    pub const fn line_ordinal(self) -> u64 {
        self.line_ordinal
    }

    #[must_use]
    pub const fn absolute_range(self) -> (u64, u64) {
        (self.absolute_start, self.absolute_end)
    }

    #[must_use]
    pub const fn metric(self) -> SourceLedgerMetric {
        self.metric
    }

    #[must_use]
    pub const fn ending(self) -> Option<CandidateLineEnding> {
        self.ending
    }

    #[must_use]
    pub const fn atom_count(self) -> u64 {
        self.atom_count
    }

    /// Diagnostics only. Exact replay rests on the same immutable root/range
    /// and shared decoder kernel, not on this non-cryptographic checksum.
    #[must_use]
    pub const fn atom_debug_digest(self) -> u64 {
        self.atom_debug_digest
    }
}

/// Grammar branch whose recognition may span physical lines. This identifies
/// the parser-owned replay recipe family, including candidates that the
/// grammar later rejects. It does not classify source at the ledger sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateRecognitionRangeKind {
    ReferenceDefinitionPrefix,
}

/// One O(1) recognized-range summary. No per-line receipt, atom list, cell-cut
/// list, or source text is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateRecognitionRangeReceipt {
    descriptor: SourceSnapshotDescriptor,
    build: ArenaBuildId,
    kind: CandidateRecognitionRangeKind,
    first_line: u64,
    line_count: u64,
    absolute_start: u64,
    absolute_end: u64,
    metric: SourceLedgerMetric,
    atom_count: u64,
    atom_debug_digest: u64,
}

impl CandidateRecognitionRangeReceipt {
    #[must_use]
    pub const fn source(self) -> SourceSnapshotDescriptor {
        self.descriptor
    }

    #[must_use]
    pub const fn build_id(self) -> ArenaBuildId {
        self.build
    }

    #[must_use]
    pub const fn kind(self) -> CandidateRecognitionRangeKind {
        self.kind
    }

    #[must_use]
    pub const fn first_line(self) -> u64 {
        self.first_line
    }

    #[must_use]
    pub const fn line_count(self) -> u64 {
        self.line_count
    }

    #[must_use]
    pub const fn absolute_range(self) -> (u64, u64) {
        (self.absolute_start, self.absolute_end)
    }

    #[must_use]
    pub const fn metric(self) -> SourceLedgerMetric {
        self.metric
    }

    #[must_use]
    pub const fn atom_count(self) -> u64 {
        self.atom_count
    }

    #[must_use]
    pub const fn atom_debug_digest(self) -> u64 {
        self.atom_debug_digest
    }
}

/// Stable semantic owner derived from a candidate-scoped fresh block permit.
/// The handle is not cloneable and cannot be constructed from a `BlockId`.
#[must_use = "an open binding must remain on the source ledger path or be closed"]
#[derive(PartialEq, Eq)]
pub struct CandidateOpenBinding {
    stamp: BindingStamp,
}

impl CandidateOpenBinding {
    #[must_use]
    pub const fn kind(&self) -> GreenKind {
        self.stamp.kind
    }

    /// Query identity only; claim APIs accept this binding, never a raw ID.
    #[must_use]
    pub const fn block_id(&self) -> BlockId {
        self.stamp.block
    }
}

impl fmt::Debug for CandidateOpenBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateOpenBinding")
            .field("kind", &self.stamp.kind)
            .field("block", &self.stamp.block)
            .field("depth", &self.stamp.depth)
            .field("build", &self.stamp.build)
            .finish_non_exhaustive()
    }
}

/// One-use semantic identity retired by a writer-minted logical-terminal
/// replacement. It can be reopened only under the same still-open parent and
/// only by the ledger which minted it; callers never receive a constructor or
/// a raw-ID reopen seam.
#[must_use = "a deferred normalization identity must resolve whole or reopen its residual"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DeferredNormalizationIdentity {
    build: ArenaBuildId,
    retired: BindingStamp,
    replacement: BindingStamp,
    parent: BindingStamp,
}

#[must_use = "whole normalization identity authority must be consumed by packed green"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ResolvedWholeNormalizationIdentity {
    build: ArenaBuildId,
    retired_block: BlockId,
    replacement_block: BlockId,
    kind: GreenKind,
}

impl ResolvedWholeNormalizationIdentity {
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    pub(crate) const fn retired_block(&self) -> BlockId {
        self.retired_block
    }

    pub(crate) const fn replacement_block(&self) -> BlockId {
        self.replacement_block
    }

    pub(crate) const fn kind(&self) -> GreenKind {
        self.kind
    }
}

impl DeferredNormalizationIdentity {
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    pub(crate) const fn retired_block(&self) -> BlockId {
        self.retired.block
    }

    pub(crate) const fn replacement_block(&self) -> BlockId {
        self.replacement.block
    }

    pub(crate) const fn survivor_kind(&self) -> GreenKind {
        self.retired.kind
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BindingStamp {
    build: ArenaBuildId,
    block: BlockId,
    kind: GreenKind,
    depth: u32,
    path_generation: u64,
}

/// Parser-selected recipe for one exact, source-ledger-bounded replay range.
///
/// The physical owner is supplied separately. Recipes that produce logical
/// text use that same binding as their target; `None` deliberately has no
/// logical target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateWriterRangeRecipe {
    None,
    Identity,
    Hidden { affinity: GreenAffinity },
    CanonicalText,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidateRangeReplayLogical {
    None,
    Identity {
        target: BindingStamp,
    },
    Hidden {
        target: BindingStamp,
        affinity: GreenAffinity,
    },
    CanonicalText {
        target: BindingStamp,
    },
}

/// Opaque source-ledger plan for one exact parser command range.
///
/// The caller supplies only a physical byte length. The ledger binds that
/// length to the current source/build/line, next unclaimed offset, physical
/// owner, logical target, and parser-recognized line extent before scanning
/// starts. Boundaries and metrics remain decoder-minted capabilities.
#[derive(Debug)]
pub(crate) struct CandidateRangeReplayPlan {
    descriptor: SourceSnapshotDescriptor,
    build: ArenaBuildId,
    line_ordinal: u64,
    absolute_start: u64,
    absolute_end: u64,
    metric_at_start: SourceLedgerMetric,
    special_at_start: SpecialCounts,
    owner: BindingStamp,
    part: CoveragePart,
    logical: CandidateRangeReplayLogical,
}

impl CandidateRangeReplayPlan {
    pub(crate) const fn absolute_range(&self) -> (u64, u64) {
        (self.absolute_start, self.absolute_end)
    }

    pub(crate) const fn recipe(&self) -> CandidateWriterRangeRecipe {
        match self.logical {
            CandidateRangeReplayLogical::None => CandidateWriterRangeRecipe::None,
            CandidateRangeReplayLogical::Identity { .. } => CandidateWriterRangeRecipe::Identity,
            CandidateRangeReplayLogical::Hidden { affinity, .. } => {
                CandidateWriterRangeRecipe::Hidden { affinity }
            }
            CandidateRangeReplayLogical::CanonicalText { .. } => {
                CandidateWriterRangeRecipe::CanonicalText
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CandidateRangeReplaySourceReceipt {
    descriptor: SourceSnapshotDescriptor,
    build: ArenaBuildId,
    line_ordinal: u64,
    absolute_start: u64,
    absolute_end: u64,
    metric: SourceLedgerMetric,
}

impl CandidateRangeReplaySourceReceipt {
    pub(crate) const fn source(self) -> SourceSnapshotDescriptor {
        self.descriptor
    }

    pub(crate) const fn build_id(self) -> ArenaBuildId {
        self.build
    }

    pub(crate) const fn line_ordinal(self) -> u64 {
        self.line_ordinal
    }

    pub(crate) const fn absolute_range(self) -> (u64, u64) {
        (self.absolute_start, self.absolute_end)
    }

    pub(crate) const fn metric(self) -> SourceLedgerMetric {
        self.metric
    }
}

fn is_known_kind(kind: GreenKind) -> bool {
    matches!(
        kind,
        GreenKind::DOCUMENT
            | GreenKind::BLOCK_QUOTE
            | GreenKind::LIST
            | GreenKind::ITEM
            | GreenKind::PARAGRAPH
            | GreenKind::INDENTED_CODE
            | GreenKind::FENCED_CODE
            | GreenKind::HTML_BLOCK
            | GreenKind::TABLE
            | GreenKind::TABLE_ROW
            | GreenKind::TABLE_CELL
            | GreenKind::HEADING
            | GreenKind::THEMATIC_BREAK
    )
}

fn is_logical_terminal(kind: GreenKind) -> bool {
    matches!(
        kind,
        GreenKind::PARAGRAPH
            | GreenKind::HEADING
            | GreenKind::TABLE_CELL
            | GreenKind::INDENTED_CODE
            | GreenKind::FENCED_CODE
            | GreenKind::HTML_BLOCK
    )
}

/// Typed physical-to-logical atom transform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateAtomicProjection {
    TabToSpaces { spaces: u8 },
    CrLfToLf,
    LoneCrToLf,
    NulToReplacement,
}

/// Parser-selected logical action whose target and atomic source are private
/// source-ledger capabilities.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CandidateLogicalAction {
    kind: CandidateLogicalActionKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidateLogicalActionKind {
    None,
    Identity {
        target: BindingStamp,
    },
    CertifiedIdentity {
        target: BindingStamp,
        source: CertifiedAtom,
    },
    Hidden {
        target: BindingStamp,
        affinity: GreenAffinity,
    },
    Atomic {
        target: BindingStamp,
        source: CertifiedAtom,
        projection: CandidateAtomicProjection,
    },
}

impl CandidateLogicalAction {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            kind: CandidateLogicalActionKind::None,
        }
    }

    pub fn identity(target: &CandidateOpenBinding) -> Result<Self, SourceBoundLedgerError> {
        require_terminal(target.stamp)?;
        Ok(Self {
            kind: CandidateLogicalActionKind::Identity {
                target: target.stamp,
            },
        })
    }

    pub fn hidden(
        target: &CandidateOpenBinding,
        affinity: GreenAffinity,
    ) -> Result<Self, SourceBoundLedgerError> {
        require_terminal(target.stamp)?;
        Ok(Self {
            kind: CandidateLogicalActionKind::Hidden {
                target: target.stamp,
                affinity,
            },
        })
    }

    fn tab_identity(
        target: &CandidateOpenBinding,
        source: &CandidateSourceAtom,
    ) -> Result<Self, SourceBoundLedgerError> {
        require_terminal(target.stamp)?;
        require_atom_kind(source, CandidateSourceAtomKind::Tab)?;
        Ok(Self {
            kind: CandidateLogicalActionKind::CertifiedIdentity {
                target: target.stamp,
                source: source.certified,
            },
        })
    }

    pub fn tab_to_spaces(
        target: &CandidateOpenBinding,
        source: &CandidateSourceAtom,
        spaces: u8,
    ) -> Result<Self, SourceBoundLedgerError> {
        require_terminal(target.stamp)?;
        if !(1..=4).contains(&spaces) {
            return Err(SourceBoundLedgerError::InvalidTabExpansion(spaces));
        }
        require_atom_kind(source, CandidateSourceAtomKind::Tab)?;
        Ok(Self {
            kind: CandidateLogicalActionKind::Atomic {
                target: target.stamp,
                source: source.certified,
                projection: CandidateAtomicProjection::TabToSpaces { spaces },
            },
        })
    }

    pub fn nul_to_replacement(
        target: &CandidateOpenBinding,
        source: &CandidateSourceAtom,
    ) -> Result<Self, SourceBoundLedgerError> {
        require_terminal(target.stamp)?;
        require_atom_kind(source, CandidateSourceAtomKind::Nul)?;
        Ok(Self {
            kind: CandidateLogicalActionKind::Atomic {
                target: target.stamp,
                source: source.certified,
                projection: CandidateAtomicProjection::NulToReplacement,
            },
        })
    }

    pub fn canonical_line_ending(
        target: &CandidateOpenBinding,
        source: &CandidateSourceAtom,
    ) -> Result<Self, SourceBoundLedgerError> {
        require_terminal(target.stamp)?;
        let CandidateSourceAtomKind::LineEnding(ending) = source.kind() else {
            return Err(SourceBoundLedgerError::WrongAtomicSource);
        };
        let projection = match ending {
            CandidateLineEnding::Lf => {
                return Ok(Self {
                    kind: CandidateLogicalActionKind::CertifiedIdentity {
                        target: target.stamp,
                        source: source.certified,
                    },
                });
            }
            CandidateLineEnding::LoneCr => CandidateAtomicProjection::LoneCrToLf,
            CandidateLineEnding::CrLf => CandidateAtomicProjection::CrLfToLf,
        };
        Ok(Self {
            kind: CandidateLogicalActionKind::Atomic {
                target: target.stamp,
                source: source.certified,
                projection,
            },
        })
    }

    fn target(self) -> Option<BindingStamp> {
        match self.kind {
            CandidateLogicalActionKind::None => None,
            CandidateLogicalActionKind::Identity { target }
            | CandidateLogicalActionKind::CertifiedIdentity { target, .. }
            | CandidateLogicalActionKind::Hidden { target, .. }
            | CandidateLogicalActionKind::Atomic { target, .. } => Some(target),
        }
    }

    fn retained(self) -> ValidatedLogicalAction {
        match self.kind {
            CandidateLogicalActionKind::None => ValidatedLogicalAction {
                kind: ValidatedLogicalKind::None,
                target: None,
                target_depth: None,
                projection: None,
                hidden_affinity: None,
            },
            CandidateLogicalActionKind::Identity { target }
            | CandidateLogicalActionKind::CertifiedIdentity { target, .. } => {
                ValidatedLogicalAction {
                    kind: ValidatedLogicalKind::Identity,
                    target: Some(target.block),
                    target_depth: Some(target.depth),
                    projection: None,
                    hidden_affinity: None,
                }
            }
            CandidateLogicalActionKind::Hidden { target, affinity } => ValidatedLogicalAction {
                kind: ValidatedLogicalKind::Hidden,
                target: Some(target.block),
                target_depth: Some(target.depth),
                projection: None,
                hidden_affinity: Some(affinity),
            },
            CandidateLogicalActionKind::Atomic {
                target, projection, ..
            } => ValidatedLogicalAction {
                kind: ValidatedLogicalKind::Atomic,
                target: Some(target.block),
                target_depth: Some(target.depth),
                projection: Some(projection),
                hidden_affinity: None,
            },
        }
    }
}

impl fmt::Debug for CandidateLogicalAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            CandidateLogicalActionKind::None => formatter.write_str("None"),
            CandidateLogicalActionKind::Identity { target } => formatter
                .debug_tuple("Identity")
                .field(&target.block)
                .finish(),
            CandidateLogicalActionKind::CertifiedIdentity { target, .. } => formatter
                .debug_tuple("CertifiedIdentity")
                .field(&target.block)
                .finish(),
            CandidateLogicalActionKind::Hidden { target, affinity } => formatter
                .debug_struct("Hidden")
                .field("target", &target.block)
                .field("affinity", &affinity)
                .finish(),
            CandidateLogicalActionKind::Atomic {
                target, projection, ..
            } => formatter
                .debug_struct("Atomic")
                .field("target", &target.block)
                .field("projection", &projection)
                .finish(),
        }
    }
}

fn require_terminal(binding: BindingStamp) -> Result<(), SourceBoundLedgerError> {
    if is_logical_terminal(binding.kind) {
        Ok(())
    } else {
        Err(SourceBoundLedgerError::LogicalTargetIsNotTerminal(
            binding.kind,
        ))
    }
}

fn require_atom_kind(
    atom: &CandidateSourceAtom,
    expected: CandidateSourceAtomKind,
) -> Result<(), SourceBoundLedgerError> {
    if atom.kind() == expected {
        Ok(())
    } else {
        Err(SourceBoundLedgerError::WrongAtomicSource)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidatedLogicalKind {
    None,
    Identity,
    Hidden,
    Atomic,
}

/// Compact logical action retained by a validated debug claim. This is output
/// data, not parser authority; the packed sink will consume the original
/// private binding/atom checks directly rather than replay this record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedLogicalAction {
    kind: ValidatedLogicalKind,
    target: Option<BlockId>,
    target_depth: Option<u32>,
    projection: Option<CandidateAtomicProjection>,
    hidden_affinity: Option<GreenAffinity>,
}

impl ValidatedLogicalAction {
    #[must_use]
    pub const fn kind(self) -> ValidatedLogicalKind {
        self.kind
    }

    #[must_use]
    pub const fn target(self) -> Option<BlockId> {
        self.target
    }

    #[must_use]
    pub const fn target_depth(self) -> Option<u32> {
        self.target_depth
    }

    #[must_use]
    pub const fn projection(self) -> Option<CandidateAtomicProjection> {
        self.projection
    }

    #[must_use]
    pub const fn hidden_affinity(self) -> Option<GreenAffinity> {
        self.hidden_affinity
    }
}

/// One production source-consumption result. This is the sole output of the
/// authoritative ledger validation: it carries exact source and structural
/// meaning, but deliberately has no packed-storage identity.
///
/// The value is non-cloneable. Moving it into the source-bound projection
/// composer is what prevents the same exact source interval from authorizing
/// two retained runs.
#[must_use = "consumed source must enter the active composer or be discarded with its candidate"]
#[derive(PartialEq, Eq)]
pub struct ConsumedSourcePiece {
    descriptor: SourceSnapshotDescriptor,
    build: ArenaBuildId,
    line_ordinal: u64,
    absolute_start: u64,
    absolute_end: u64,
    metric: SourceLedgerMetric,
    physical_owner: BindingStamp,
    owner_relative_depth: u32,
    structural_state_generation: u64,
    part: CoveragePart,
    logical: ValidatedLogicalAction,
}

impl ConsumedSourcePiece {
    #[must_use]
    pub const fn source(&self) -> SourceSnapshotDescriptor {
        self.descriptor
    }

    #[must_use]
    pub const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    #[must_use]
    pub const fn line_ordinal(&self) -> u64 {
        self.line_ordinal
    }

    #[must_use]
    pub const fn absolute_range(&self) -> (u64, u64) {
        (self.absolute_start, self.absolute_end)
    }

    #[must_use]
    pub const fn metric(&self) -> SourceLedgerMetric {
        self.metric
    }

    #[must_use]
    pub const fn owner_block(&self) -> BlockId {
        self.physical_owner.block
    }

    #[must_use]
    pub const fn owner_relative_depth(&self) -> u32 {
        self.owner_relative_depth
    }

    #[must_use]
    pub const fn part(&self) -> CoveragePart {
        self.part
    }

    #[must_use]
    pub const fn logical(&self) -> ValidatedLogicalAction {
        self.logical
    }

    pub(crate) const fn physical_owner_stamp(&self) -> BindingStamp {
        self.physical_owner
    }

    pub(crate) const fn structural_state_generation(&self) -> u64 {
        self.structural_state_generation
    }

    fn fold_digest(&self, mut digest: u64) -> u64 {
        fold_u64(&mut digest, self.line_ordinal);
        fold_u64(&mut digest, self.absolute_start);
        fold_u64(&mut digest, self.absolute_end);
        fold_u64(&mut digest, self.metric.bytes);
        fold_u64(&mut digest, self.metric.utf16);
        fold_u64(&mut digest, self.physical_owner.block.0);
        fold_u64(&mut digest, self.physical_owner.path_generation);
        fold_u64(&mut digest, self.structural_state_generation);
        fold_byte(&mut digest, self.physical_owner.kind.0);
        fold_byte(&mut digest, self.part.0);
        digest
    }
}

fn logical_metric_update(
    piece: &ConsumedSourcePiece,
    path: &[SourceLedgerMetric],
) -> Result<Option<(usize, SourceLedgerMetric)>, SourceBoundLedgerError> {
    let logical = piece.logical();
    let Some(target_depth) = logical.target_depth() else {
        if logical.kind() == ValidatedLogicalKind::None {
            return Ok(None);
        }
        return Err(SourceBoundLedgerError::Invariant(
            "logical contribution has no target depth",
        ));
    };
    let depth =
        usize::try_from(target_depth).map_err(|_| SourceBoundLedgerError::BindingNotOpen)?;
    let current = path
        .get(depth)
        .copied()
        .ok_or(SourceBoundLedgerError::BindingNotOpen)?;
    let delta = match logical.kind() {
        ValidatedLogicalKind::None => {
            return Err(SourceBoundLedgerError::Invariant(
                "physical-only contribution has a logical target",
            ));
        }
        ValidatedLogicalKind::Identity => piece.metric(),
        ValidatedLogicalKind::Hidden => SourceLedgerMetric::default(),
        ValidatedLogicalKind::Atomic => {
            match logical
                .projection()
                .ok_or(SourceBoundLedgerError::Invariant(
                    "atomic logical contribution has no projection",
                ))? {
                CandidateAtomicProjection::TabToSpaces { spaces } => SourceLedgerMetric {
                    bytes: u64::from(spaces),
                    utf16: u64::from(spaces),
                },
                CandidateAtomicProjection::CrLfToLf | CandidateAtomicProjection::LoneCrToLf => {
                    SourceLedgerMetric { bytes: 1, utf16: 1 }
                }
                CandidateAtomicProjection::NulToReplacement => {
                    SourceLedgerMetric { bytes: 3, utf16: 1 }
                }
            }
        }
    };
    Ok(Some((depth, current.checked_add(delta)?)))
}

impl fmt::Debug for ConsumedSourcePiece {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConsumedSourcePiece")
            .field("source", &self.descriptor)
            .field("build", &self.build)
            .field("line", &self.line_ordinal)
            .field("range", &(self.absolute_start..self.absolute_end))
            .field("metric", &self.metric)
            .field("physical_owner", &self.physical_owner.block)
            .field("owner_relative_depth", &self.owner_relative_depth)
            .field(
                "structural_state_generation",
                &self.structural_state_generation,
            )
            .field("part", &self.part)
            .field("logical", &self.logical)
            .finish_non_exhaustive()
    }
}

/// Proof-harness wrapper around the exact same production source piece. Its
/// coverage permit and outer affinity are debug observations only. Physical
/// lookup is fully determined by exact range, owner, part, and structural
/// position; logical Hidden affinity is retained separately in `logical`.
/// Neither debug-only field enters production projection composition.
#[derive(PartialEq, Eq)]
pub struct ValidatedSourceClaim {
    piece: ConsumedSourcePiece,
    coverage: CoverageId,
    affinity: GreenAffinity,
}

impl ValidatedSourceClaim {
    #[must_use]
    pub const fn source(&self) -> SourceSnapshotDescriptor {
        self.piece.descriptor
    }

    #[must_use]
    pub const fn build_id(&self) -> ArenaBuildId {
        self.piece.build
    }

    #[must_use]
    pub const fn coverage_id(&self) -> CoverageId {
        self.coverage
    }

    #[must_use]
    pub const fn line_ordinal(&self) -> u64 {
        self.piece.line_ordinal
    }

    #[must_use]
    pub const fn absolute_range(&self) -> (u64, u64) {
        (self.piece.absolute_start, self.piece.absolute_end)
    }

    #[must_use]
    pub const fn metric(&self) -> SourceLedgerMetric {
        self.piece.metric
    }

    #[must_use]
    pub const fn owner_block(&self) -> BlockId {
        self.piece.physical_owner.block
    }

    #[must_use]
    pub const fn part(&self) -> CoveragePart {
        self.piece.part
    }

    #[must_use]
    pub const fn logical(&self) -> ValidatedLogicalAction {
        self.piece.logical
    }

    #[must_use]
    pub const fn affinity(&self) -> GreenAffinity {
        self.affinity
    }

    #[must_use]
    pub const fn golden_debug(&self) -> GoldenSourceClaimDebug<'_> {
        GoldenSourceClaimDebug(self)
    }
}

impl fmt::Debug for ValidatedSourceClaim {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.golden_debug().fmt(formatter)
    }
}

pub struct GoldenSourceClaimDebug<'claim>(&'claim ValidatedSourceClaim);

impl fmt::Display for GoldenSourceClaimDebug<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let claim = self.0;
        write!(
            formatter,
            "rev={} root={} build={:?} line={} abs={}..{} metric={}b/{}u16 coverage={} owner={}:{} part={} logical={:?} affinity={:?}",
            claim.piece.descriptor.revision.0,
            claim.piece.descriptor.root.0,
            claim.piece.build,
            claim.piece.line_ordinal,
            claim.piece.absolute_start,
            claim.piece.absolute_end,
            claim.piece.metric.bytes,
            claim.piece.metric.utf16,
            claim.coverage.0,
            claim.piece.physical_owner.kind.0,
            claim.piece.physical_owner.block.0,
            claim.piece.part.0,
            claim.piece.logical,
            claim.affinity,
        )
    }
}

impl fmt::Debug for GoldenSourceClaimDebug<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingSourceKind {
    Gap,
    Terminator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateTerminatorResolution {
    ContinueCanonicalNewline,
    CloseNone,
    CloseCanonicalNewline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateLineReceipt {
    descriptor: SourceSnapshotDescriptor,
    build: ArenaBuildId,
    line_ordinal: u64,
    absolute_start: u64,
    absolute_end: u64,
    metric: SourceLedgerMetric,
    ending: Option<CandidateLineEnding>,
    pending: Option<PendingSourceKind>,
    atom_count: u64,
    atom_debug_digest: u64,
    recognition_replay_matched: bool,
}

impl CandidateLineReceipt {
    #[must_use]
    pub const fn line_ordinal(self) -> u64 {
        self.line_ordinal
    }

    #[must_use]
    pub const fn absolute_range(self) -> (u64, u64) {
        (self.absolute_start, self.absolute_end)
    }

    #[must_use]
    pub const fn metric(self) -> SourceLedgerMetric {
        self.metric
    }

    #[must_use]
    pub const fn ending(self) -> Option<CandidateLineEnding> {
        self.ending
    }

    #[must_use]
    pub const fn pending(self) -> Option<PendingSourceKind> {
        self.pending
    }

    #[must_use]
    pub const fn atom_count(self) -> u64 {
        self.atom_count
    }

    /// Debug-only checksum of the shared decoder's atom kind/range sequence.
    #[must_use]
    pub const fn atom_debug_digest(self) -> u64 {
        self.atom_debug_digest
    }

    /// True only when the same immutable source range was first traversed by
    /// the recognition role and then replayed through the shared decoder by
    /// the authoritative role with every structural receipt field equal.
    #[must_use]
    pub const fn recognition_replay_matched(self) -> bool {
        self.recognition_replay_matched
    }
}

/// Non-cloneable exact EOF seal. It is suitable for a later manifest builder;
/// the digest getter below remains debug-only and is never adoption authority.
#[must_use = "the exact source seal must be consumed by candidate composition or discarded"]
pub struct CandidateSourceSeal {
    descriptor: SourceSnapshotDescriptor,
    build: ArenaBuildId,
    metric: SourceLedgerMetric,
    line_count: u64,
    claim_count: u64,
    debug_digest: u64,
    maximum_decoder_bytes: usize,
    source_chunk_loads: usize,
    source_bytes_copied: usize,
    maximum_source_chunk_bytes: usize,
    recognition_source_chunk_loads: usize,
    recognition_source_bytes_copied: usize,
    recognition_maximum_source_chunk_bytes: usize,
    recognition_maximum_decoder_bytes: usize,
    recognition_maximum_lead_bytes: u64,
    authoritative_root_utf16: u64,
    maximum_open_path_len: usize,
    maximum_open_path_capacity_bytes: usize,
}

impl CandidateSourceSeal {
    #[must_use]
    pub const fn source(&self) -> SourceSnapshotDescriptor {
        self.descriptor
    }

    #[must_use]
    pub const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    #[must_use]
    pub const fn metric(&self) -> SourceLedgerMetric {
        self.metric
    }

    #[must_use]
    pub const fn line_count(&self) -> u64 {
        self.line_count
    }

    #[must_use]
    pub const fn claim_count(&self) -> u64 {
        self.claim_count
    }

    /// Production name for the exact number of consumed source pieces. The
    /// adjacent `claim_count` getter is retained for Stage-1 debug fixtures.
    #[must_use]
    pub const fn source_piece_count(&self) -> u64 {
        self.claim_count
    }

    /// Deterministic diagnostics only; this value is not source, lineage, or
    /// publication authority.
    #[must_use]
    pub const fn debug_digest(&self) -> u64 {
        self.debug_digest
    }

    #[must_use]
    pub const fn maximum_decoder_bytes(&self) -> usize {
        self.maximum_decoder_bytes
    }

    #[must_use]
    pub const fn source_chunk_loads(&self) -> usize {
        self.source_chunk_loads
    }

    #[must_use]
    pub const fn source_bytes_copied(&self) -> usize {
        self.source_bytes_copied
    }

    #[must_use]
    pub const fn maximum_source_chunk_bytes(&self) -> usize {
        self.maximum_source_chunk_bytes
    }

    #[must_use]
    pub const fn recognition_source_chunk_loads(&self) -> usize {
        self.recognition_source_chunk_loads
    }

    #[must_use]
    pub const fn recognition_source_bytes_copied(&self) -> usize {
        self.recognition_source_bytes_copied
    }

    #[must_use]
    pub const fn recognition_maximum_source_chunk_bytes(&self) -> usize {
        self.recognition_maximum_source_chunk_bytes
    }

    #[must_use]
    pub const fn recognition_maximum_decoder_bytes(&self) -> usize {
        self.recognition_maximum_decoder_bytes
    }

    #[must_use]
    pub const fn recognition_maximum_lead_bytes(&self) -> u64 {
        self.recognition_maximum_lead_bytes
    }

    /// O(1) Crop root metric bound by the actor before parsing. This is
    /// independent of the replay-derived parsed-range total.
    #[must_use]
    pub const fn authoritative_root_utf16(&self) -> u64 {
        self.authoritative_root_utf16
    }

    #[must_use]
    pub const fn maximum_open_path_len(&self) -> usize {
        self.maximum_open_path_len
    }

    #[must_use]
    pub const fn maximum_open_path_capacity_bytes(&self) -> usize {
        self.maximum_open_path_capacity_bytes
    }
}

impl fmt::Debug for CandidateSourceSeal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateSourceSeal")
            .field("source", &self.descriptor)
            .field("build", &self.build)
            .field("metric", &self.metric)
            .field("line_count", &self.line_count)
            .field("claim_count", &self.claim_count)
            .field("debug_digest", &format_args!("{:#018x}", self.debug_digest))
            .field("maximum_decoder_bytes", &self.maximum_decoder_bytes)
            .field("source_chunk_loads", &self.source_chunk_loads)
            .field("source_bytes_copied", &self.source_bytes_copied)
            .field(
                "maximum_source_chunk_bytes",
                &self.maximum_source_chunk_bytes,
            )
            .field(
                "recognition_source_chunk_loads",
                &self.recognition_source_chunk_loads,
            )
            .field(
                "recognition_source_bytes_copied",
                &self.recognition_source_bytes_copied,
            )
            .field(
                "recognition_maximum_source_chunk_bytes",
                &self.recognition_maximum_source_chunk_bytes,
            )
            .field(
                "recognition_maximum_decoder_bytes",
                &self.recognition_maximum_decoder_bytes,
            )
            .field(
                "recognition_maximum_lead_bytes",
                &self.recognition_maximum_lead_bytes,
            )
            .field("authoritative_root_utf16", &self.authoritative_root_utf16)
            .field("maximum_open_path_len", &self.maximum_open_path_len)
            .field(
                "maximum_open_path_capacity_bytes",
                &self.maximum_open_path_capacity_bytes,
            )
            .finish()
    }
}

/// Non-cloneable candidate source completion that distinguishes replayed
/// prefix work from an unchanged, lineage-proven suffix.
///
/// This is intentionally not [`CandidateSourceSeal`]: the decoder did not
/// visit EOF and `replayed_source_piece_count`/`prefix_debug_digest` describe
/// only the freshly replayed prefix. Final source metric and physical-line
/// count are storage-derived facts carried by the consumed tail authority.
/// The value retains no Crop cursor, source root, source bytes, or open path.
#[must_use = "the adopted source seal must enter the matching composer or be discarded"]
pub(crate) struct CandidateAdoptedSourceSeal {
    descriptor: SourceSnapshotDescriptor,
    build: ArenaBuildId,
    accepted_projection_prefix_metric: SourceLedgerMetric,
    physical_parser_prefix_metric: SourceLedgerMetric,
    metric: SourceLedgerMetric,
    replayed_prefix_line_count: u64,
    line_count: u64,
    replayed_source_piece_count: u64,
    prefix_debug_digest: u64,
    maximum_decoder_bytes: usize,
    source_chunk_loads: usize,
    source_bytes_copied: usize,
    maximum_source_chunk_bytes: usize,
    recognition_source_chunk_loads: usize,
    recognition_source_bytes_copied: usize,
    recognition_maximum_source_chunk_bytes: usize,
    recognition_maximum_decoder_bytes: usize,
    recognition_maximum_lead_bytes: u64,
    authoritative_root_utf16: u64,
    maximum_open_path_len: usize,
    maximum_open_path_capacity_bytes: usize,
}

impl CandidateAdoptedSourceSeal {
    pub(crate) const fn source(&self) -> SourceSnapshotDescriptor {
        self.descriptor
    }

    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    pub(crate) const fn accepted_projection_prefix_metric(&self) -> SourceLedgerMetric {
        self.accepted_projection_prefix_metric
    }

    pub(crate) const fn physical_parser_prefix_metric(&self) -> SourceLedgerMetric {
        self.physical_parser_prefix_metric
    }

    pub(crate) const fn metric(&self) -> SourceLedgerMetric {
        self.metric
    }

    pub(crate) const fn replayed_prefix_line_count(&self) -> u64 {
        self.replayed_prefix_line_count
    }

    pub(crate) const fn line_count(&self) -> u64 {
        self.line_count
    }

    /// Exact count of freshly replayed source pieces. It is never presented as
    /// a whole-document total after suffix adoption.
    pub(crate) const fn replayed_source_piece_count(&self) -> u64 {
        self.replayed_source_piece_count
    }

    /// Prefix-only diagnostics; never source or adoption authority.
    pub(crate) const fn prefix_debug_digest(&self) -> u64 {
        self.prefix_debug_digest
    }

    pub(crate) const fn source_bytes_copied(&self) -> usize {
        self.source_bytes_copied
    }

    pub(crate) const fn recognition_source_bytes_copied(&self) -> usize {
        self.recognition_source_bytes_copied
    }

    #[cfg(test)]
    pub(crate) const fn retained_source_bytes_for_test(&self) -> usize {
        0
    }
}

impl fmt::Debug for CandidateAdoptedSourceSeal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateAdoptedSourceSeal")
            .field("source", &self.descriptor)
            .field("build", &self.build)
            .field(
                "accepted_projection_prefix_metric",
                &self.accepted_projection_prefix_metric,
            )
            .field(
                "physical_parser_prefix_metric",
                &self.physical_parser_prefix_metric,
            )
            .field("metric", &self.metric)
            .field(
                "replayed_prefix_line_count",
                &self.replayed_prefix_line_count,
            )
            .field("line_count", &self.line_count)
            .field(
                "replayed_source_piece_count",
                &self.replayed_source_piece_count,
            )
            .field(
                "prefix_debug_digest",
                &format_args!("{:#018x}", self.prefix_debug_digest),
            )
            .field("maximum_decoder_bytes", &self.maximum_decoder_bytes)
            .field("source_chunk_loads", &self.source_chunk_loads)
            .field("source_bytes_copied", &self.source_bytes_copied)
            .field(
                "maximum_source_chunk_bytes",
                &self.maximum_source_chunk_bytes,
            )
            .field(
                "recognition_source_chunk_loads",
                &self.recognition_source_chunk_loads,
            )
            .field(
                "recognition_source_bytes_copied",
                &self.recognition_source_bytes_copied,
            )
            .field(
                "recognition_maximum_source_chunk_bytes",
                &self.recognition_maximum_source_chunk_bytes,
            )
            .field(
                "recognition_maximum_decoder_bytes",
                &self.recognition_maximum_decoder_bytes,
            )
            .field(
                "recognition_maximum_lead_bytes",
                &self.recognition_maximum_lead_bytes,
            )
            .field("authoritative_root_utf16", &self.authoritative_root_utf16)
            .field("maximum_open_path_len", &self.maximum_open_path_len)
            .field(
                "maximum_open_path_capacity_bytes",
                &self.maximum_open_path_capacity_bytes,
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceBoundLedgerError {
    WrongEpoch,
    WrongSourceRoot,
    WrongSourceOffset,
    SourceLengthOverflow,
    InvalidUtf8,
    IncompleteUtf8AtEof,
    InvalidKind(GreenKind),
    LogicalTargetIsNotTerminal(GreenKind),
    InvalidCoveragePart(CoveragePart),
    InvalidTabExpansion(u8),
    WrongAtomicSource,
    WrongBoundary,
    BoundaryFromAnotherLine,
    OutOfOrderClaim,
    EmptyClaim,
    IdentityCrossesTypedAtom,
    WrongBindingBuild,
    BindingNotOpen,
    CloseIsNotTop,
    LineAlreadyEnded,
    LineNotEnded,
    LineCoverageIncomplete,
    RecognitionReplayPending,
    RecognitionLineNotAtStart,
    RecognitionLineNotEnded,
    RecognitionReplayMismatch,
    RecognitionRangeAlreadyOpen,
    RecognitionRangeNotOpen,
    RecognitionByteSessionAlreadyOpen,
    RecognitionByteSessionNotOpen,
    RecognitionByteWrongSession,
    RecognitionByteEmptyBareEof,
    RecognitionByteLineMismatch,
    RecognitionByteSessionIncomplete,
    RecognitionByteSessionFailed,
    RecognitionByteOutOfOrder,
    RecognitionBytePastLine,
    RecognitionByteCrossedLine,
    RangeReplayEmpty,
    RangeReplayUnavailable,
    RangeReplayWrongPlan,
    RangeReplayEndpointOutsideLine,
    RangeReplayEndpointSplitsAtom,
    RangeReplayUnexpectedAtom,
    RangeReplayIncomplete,
    LineBoundaryContinuationUnavailable,
    ResumeOffsetIsNotPhysicalLineStart,
    RootUtf16Mismatch,
    PreviousPendingUnresolved,
    PendingAlreadyStaged,
    NoPendingTerminator,
    NoPendingGap,
    CurrentLineIsNotBlank,
    PendingGapAffinityMismatch,
    PendingGapOwnerOpenedAfterGap,
    EofNotObserved,
    OpenBindingsAtSeal,
    TailAdoptionMismatch,
    AlreadySealed,
    Overflow(&'static str),
    Invariant(&'static str),
}

impl fmt::Display for SourceBoundLedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "candidate source-ledger error: {self:?}")
    }
}

impl std::error::Error for SourceBoundLedgerError {}

#[derive(Clone, Copy, Debug)]
struct DecodeState {
    bytes: [u8; 4],
    len: u8,
    expected: u8,
    start: u64,
}

impl DecodeState {
    const fn new(start: u64, first: u8, expected: u8) -> Self {
        let mut bytes = [0; 4];
        bytes[0] = first;
        Self {
            bytes,
            len: 1,
            expected,
            start,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PendingCr {
    start: u64,
    metric_before: SourceLedgerMetric,
    special_before: SpecialCounts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedAtom {
    absolute_start: u64,
    absolute_end: u64,
    metric_after: SourceLedgerMetric,
    special_after: SpecialCounts,
    kind: CandidateSourceAtomKind,
}

#[derive(Debug, PartialEq, Eq)]
enum AtomDecoderPoll {
    NeedFuel(CandidateSourcePollReceipt),
    Atom {
        atom: DecodedAtom,
        receipt: CandidateSourcePollReceipt,
    },
    Eof(CandidateSourcePollReceipt),
}

/// One byte-sized transition through the shared decoder kernel.
///
/// A lone-CR lookahead can emit an atom while retaining the non-LF byte for
/// the next transition; in that case `exposed` is `None`. Bounded byte
/// sessions never cross that physical-line edge and therefore never expose a
/// next-line byte to the current scanner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AtomDecoderStep {
    exposed: Option<SourceByte>,
    atom: Option<DecodedAtom>,
}

fn checked_cursor_metrics_add(
    earlier: CursorMetrics,
    later: CursorMetrics,
) -> Result<CursorMetrics, SourceBoundLedgerError> {
    Ok(CursorMetrics {
        chunk_loads: earlier
            .chunk_loads
            .checked_add(later.chunk_loads)
            .ok_or(SourceBoundLedgerError::Overflow("source chunk loads"))?,
        chunk_bytes_copied: earlier
            .chunk_bytes_copied
            .checked_add(later.chunk_bytes_copied)
            .ok_or(SourceBoundLedgerError::Overflow(
                "source chunk bytes copied",
            ))?,
        maximum_chunk_bytes: earlier.maximum_chunk_bytes.max(later.maximum_chunk_bytes),
    })
}

/// Shared UTF-8/CRLF/tab/NUL kernel used by both source-cursor roles. Neither
/// role has a second byte classifier.
#[derive(Debug)]
struct AtomDecoder {
    descriptor: SourceSnapshotDescriptor,
    cursor: CropSourceCursor,
    read_offset: u64,
    emitted_offset: u64,
    metric: SourceLedgerMetric,
    special: SpecialCounts,
    decode: Option<DecodeState>,
    pending_cr: Option<PendingCr>,
    replay: Option<SourceByte>,
    cursor_metrics_before_remint: CursorMetrics,
    maximum_decoder_bytes: usize,
    eof_observed: bool,
}

impl AtomDecoder {
    fn new(descriptor: SourceSnapshotDescriptor, cursor: CropSourceCursor) -> Self {
        debug_assert_eq!(descriptor.root, cursor.source_identity());
        Self {
            descriptor,
            cursor,
            read_offset: 0,
            emitted_offset: 0,
            metric: SourceLedgerMetric::default(),
            special: SpecialCounts::default(),
            decode: None,
            pending_cr: None,
            replay: None,
            cursor_metrics_before_remint: CursorMetrics::default(),
            maximum_decoder_bytes: 0,
            eof_observed: false,
        }
    }

    /// Reconstructs decoder control at a certified emitted atom boundary.
    ///
    /// The cursor begins at `absolute_offset`; no partial UTF-8 bytes, pending
    /// CR, or lone-CR replay byte crosses the seam. A lone-CR lookahead byte is
    /// intentionally reread from Crop after resume. Only scalar counters and
    /// prior diagnostic totals survive.
    #[allow(dead_code)] // Wired by the actor seam before the composite checkpoint lands.
    fn from_emitted_boundary(
        descriptor: SourceSnapshotDescriptor,
        cursor: CropSourceCursor,
        absolute_offset: u64,
        metric: SourceLedgerMetric,
        special: SpecialCounts,
        state: DecoderLineBoundaryState,
    ) -> Self {
        debug_assert_eq!(descriptor.root, cursor.source_identity());
        debug_assert_eq!(usize::try_from(absolute_offset).ok(), Some(cursor.offset()));
        Self {
            descriptor,
            cursor,
            read_offset: absolute_offset,
            emitted_offset: absolute_offset,
            metric,
            special,
            decode: None,
            pending_cr: None,
            replay: None,
            cursor_metrics_before_remint: state.cursor_metrics,
            maximum_decoder_bytes: state.maximum_decoder_bytes,
            eof_observed: state.eof_observed,
        }
    }

    fn cumulative_cursor_metrics(&self) -> Result<CursorMetrics, SourceBoundLedgerError> {
        checked_cursor_metrics_add(self.cursor_metrics_before_remint, self.cursor.metrics())
    }

    #[allow(dead_code)] // Wired by the actor seam before the composite checkpoint lands.
    fn validate_emitted_line_boundary(
        &self,
        absolute_offset: u64,
        metric: SourceLedgerMetric,
        special: SpecialCounts,
        eof_observed: bool,
    ) -> Result<(), SourceBoundLedgerError> {
        if self.descriptor.root != self.cursor.source_identity()
            || self.emitted_offset != absolute_offset
            || self.metric != metric
            || self.special != special
            || self.decode.is_some()
            || self.pending_cr.is_some()
            || self.eof_observed != eof_observed
        {
            return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
        }
        let cursor_offset = u64::try_from(self.cursor.offset())
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?;
        match self.replay {
            None => {
                if self.read_offset != absolute_offset || cursor_offset != absolute_offset {
                    return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
                }
            }
            Some(replay) => {
                let replay_offset = replay.offset_u64()?;
                if replay.root != self.descriptor.root
                    || replay_offset != absolute_offset
                    || self.read_offset
                        != absolute_offset
                            .checked_add(1)
                            .ok_or(SourceBoundLedgerError::Overflow("lone-CR replay offset"))?
                    || cursor_offset != self.read_offset
                    || eof_observed
                {
                    return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
                }
            }
        }
        let descriptor_bytes = u64::try_from(self.descriptor.bytes)
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?;
        if absolute_offset > descriptor_bytes
            || (eof_observed && absolute_offset != descriptor_bytes)
        {
            return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
        }
        let _ = self.cumulative_cursor_metrics()?;
        Ok(())
    }

    fn poll(&mut self, fuel: usize) -> Result<AtomDecoderPoll, SourceBoundLedgerError> {
        if self.eof_observed {
            return Ok(AtomDecoderPoll::Eof(CandidateSourcePollReceipt::default()));
        }
        let mut receipt = CandidateSourcePollReceipt::default();
        while receipt.work_units < fuel {
            receipt.work_units += 1;
            if self.at_physical_eof() {
                return self.finish_at_eof(receipt);
            }
            if let Some(atom) = self.poll_one(&mut receipt)?.atom {
                return Ok(AtomDecoderPoll::Atom { atom, receipt });
            }
            if self.at_physical_eof() {
                return self.finish_at_eof(receipt);
            }
        }
        Ok(AtomDecoderPoll::NeedFuel(receipt))
    }

    fn finish_at_eof(
        &mut self,
        receipt: CandidateSourcePollReceipt,
    ) -> Result<AtomDecoderPoll, SourceBoundLedgerError> {
        if self.decode.is_some() {
            return Err(SourceBoundLedgerError::IncompleteUtf8AtEof);
        }
        if let Some(pending) = self.pending_cr.take() {
            let atom = self.emit_atom(
                pending.start,
                CandidateSourceAtomKind::LineEnding(CandidateLineEnding::LoneCr),
                pending.metric_before,
                pending.special_before,
            )?;
            return Ok(AtomDecoderPoll::Atom { atom, receipt });
        }
        self.eof_observed = true;
        Ok(AtomDecoderPoll::Eof(receipt))
    }

    fn poll_one(
        &mut self,
        receipt: &mut CandidateSourcePollReceipt,
    ) -> Result<AtomDecoderStep, SourceBoundLedgerError> {
        if self.pending_cr.is_some() {
            let (exposed, atom) = self.resolve_pending_cr(receipt)?;
            return Ok(AtomDecoderStep {
                exposed,
                atom: Some(atom),
            });
        }
        let byte = self.take_source_byte(receipt)?;
        let atom = if self.decode.is_some() {
            self.continue_utf8(byte)?
        } else {
            self.start_byte(byte)?
        };
        Ok(AtomDecoderStep {
            exposed: Some(byte),
            atom,
        })
    }

    fn resolve_pending_cr(
        &mut self,
        receipt: &mut CandidateSourcePollReceipt,
    ) -> Result<(Option<SourceByte>, DecodedAtom), SourceBoundLedgerError> {
        let byte = self.take_source_byte(receipt)?;
        let pending = self.pending_cr.take().expect("pending CR was checked");
        if byte.byte == b'\n' {
            Ok((
                Some(byte),
                self.emit_atom(
                    pending.start,
                    CandidateSourceAtomKind::LineEnding(CandidateLineEnding::CrLf),
                    pending.metric_before,
                    pending.special_before,
                )?,
            ))
        } else {
            self.replay = Some(byte);
            Ok((
                None,
                self.emit_atom(
                    pending.start,
                    CandidateSourceAtomKind::LineEnding(CandidateLineEnding::LoneCr),
                    pending.metric_before,
                    pending.special_before,
                )?,
            ))
        }
    }

    /// Consumes exactly one scanner-visible byte through the same UTF-8/CRLF
    /// state machine used by ordinary scalar recognition.
    fn consume_exposed_byte(
        &mut self,
        expected_absolute_offset: u64,
    ) -> Result<(u8, Option<DecodedAtom>, CandidateSourcePollReceipt), SourceBoundLedgerError> {
        if self.eof_observed || self.at_physical_eof() {
            return Err(SourceBoundLedgerError::RecognitionBytePastLine);
        }
        let mut receipt = CandidateSourcePollReceipt {
            work_units: 1,
            source_bytes_read: 0,
        };
        let step = self.poll_one(&mut receipt)?;
        let exposed = step
            .exposed
            .ok_or(SourceBoundLedgerError::RecognitionByteCrossedLine)?;
        if exposed.root != self.descriptor.root || exposed.offset_u64()? != expected_absolute_offset
        {
            return Err(SourceBoundLedgerError::WrongSourceOffset);
        }
        Ok((exposed.byte, step.atom, receipt))
    }

    /// Closes a line at actor/index-proven bounds without speculative access
    /// to the next line. In particular a lone CR is emitted directly from the
    /// pending state instead of reading and replaying the following byte.
    fn finish_bounded_line(
        &mut self,
        absolute_end: u64,
        ending: SourcePhysicalLineEnding,
    ) -> Result<Option<DecodedAtom>, SourceBoundLedgerError> {
        if self.decode.is_some() || self.replay.is_some() || self.eof_observed {
            return Err(SourceBoundLedgerError::RecognitionByteLineMismatch);
        }
        match ending {
            SourcePhysicalLineEnding::Lf | SourcePhysicalLineEnding::CrLf => {
                if self.pending_cr.is_some()
                    || self.emitted_offset != absolute_end
                    || self.read_offset != absolute_end
                {
                    return Err(SourceBoundLedgerError::RecognitionByteLineMismatch);
                }
                Ok(None)
            }
            SourcePhysicalLineEnding::LoneCr => {
                let pending = self
                    .pending_cr
                    .take()
                    .ok_or(SourceBoundLedgerError::RecognitionByteLineMismatch)?;
                if pending.start.checked_add(1) != Some(absolute_end)
                    || self.read_offset != absolute_end
                    || self.emitted_offset != pending.start
                {
                    self.pending_cr = Some(pending);
                    return Err(SourceBoundLedgerError::RecognitionByteLineMismatch);
                }
                self.emit_atom(
                    pending.start,
                    CandidateSourceAtomKind::LineEnding(CandidateLineEnding::LoneCr),
                    pending.metric_before,
                    pending.special_before,
                )
                .map(Some)
            }
            SourcePhysicalLineEnding::BareEof => {
                if self.pending_cr.is_some()
                    || self.emitted_offset != absolute_end
                    || self.read_offset != absolute_end
                    || usize::try_from(absolute_end).ok() != Some(self.descriptor.bytes)
                    || self.cursor.offset() != self.descriptor.bytes
                {
                    return Err(SourceBoundLedgerError::RecognitionByteLineMismatch);
                }
                self.eof_observed = true;
                Ok(None)
            }
        }
    }

    fn continue_utf8(
        &mut self,
        byte: SourceByte,
    ) -> Result<Option<DecodedAtom>, SourceBoundLedgerError> {
        let mut decode = self.decode.take().expect("UTF-8 state was checked");
        if byte.byte & 0xc0 != 0x80 {
            return Err(SourceBoundLedgerError::InvalidUtf8);
        }
        decode.bytes[usize::from(decode.len)] = byte.byte;
        decode.len += 1;
        self.maximum_decoder_bytes = self.maximum_decoder_bytes.max(usize::from(decode.len));
        if decode.len < decode.expected {
            self.decode = Some(decode);
            return Ok(None);
        }
        let text = std::str::from_utf8(&decode.bytes[..usize::from(decode.expected)])
            .map_err(|_| SourceBoundLedgerError::InvalidUtf8)?;
        let scalar = text
            .chars()
            .next()
            .ok_or(SourceBoundLedgerError::InvalidUtf8)?;
        self.emit_atom(
            decode.start,
            CandidateSourceAtomKind::Scalar(scalar),
            self.metric,
            self.special,
        )
        .map(Some)
    }

    fn start_byte(
        &mut self,
        byte: SourceByte,
    ) -> Result<Option<DecodedAtom>, SourceBoundLedgerError> {
        let start = byte.offset_u64()?;
        let kind = match byte.byte {
            b'\r' => {
                self.pending_cr = Some(PendingCr {
                    start,
                    metric_before: self.metric,
                    special_before: self.special,
                });
                return Ok(None);
            }
            b'\n' => CandidateSourceAtomKind::LineEnding(CandidateLineEnding::Lf),
            b'\t' => CandidateSourceAtomKind::Tab,
            0 => CandidateSourceAtomKind::Nul,
            value if value < 0x80 => CandidateSourceAtomKind::Scalar(char::from(value)),
            first => {
                let expected = match first {
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => return Err(SourceBoundLedgerError::InvalidUtf8),
                };
                self.decode = Some(DecodeState::new(start, first, expected));
                self.maximum_decoder_bytes = self.maximum_decoder_bytes.max(1);
                return Ok(None);
            }
        };
        self.emit_atom(start, kind, self.metric, self.special)
            .map(Some)
    }

    fn take_source_byte(
        &mut self,
        receipt: &mut CandidateSourcePollReceipt,
    ) -> Result<SourceByte, SourceBoundLedgerError> {
        if let Some(byte) = self.replay.take() {
            return Ok(byte);
        }
        let byte = self
            .cursor
            .next_byte()
            .ok_or(SourceBoundLedgerError::WrongSourceOffset)?;
        if byte.root != self.descriptor.root {
            return Err(SourceBoundLedgerError::WrongSourceRoot);
        }
        let expected = usize::try_from(self.read_offset)
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?;
        if byte.offset != expected {
            return Err(SourceBoundLedgerError::WrongSourceOffset);
        }
        self.read_offset = self
            .read_offset
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow("read offset"))?;
        receipt.source_bytes_read += 1;
        Ok(byte)
    }

    fn emit_atom(
        &mut self,
        start: u64,
        kind: CandidateSourceAtomKind,
        metric_before: SourceLedgerMetric,
        special_before: SpecialCounts,
    ) -> Result<DecodedAtom, SourceBoundLedgerError> {
        if start != self.emitted_offset {
            return Err(SourceBoundLedgerError::WrongSourceOffset);
        }
        let (bytes, utf16) = atom_metric(kind)?;
        let metric_after = metric_before.checked_add(SourceLedgerMetric { bytes, utf16 })?;
        let absolute_end = start
            .checked_add(bytes)
            .ok_or(SourceBoundLedgerError::Overflow("atom end"))?;
        if absolute_end > self.read_offset {
            return Err(SourceBoundLedgerError::WrongSourceOffset);
        }
        let special_after = increment_special(special_before, kind)?;
        let atom = DecodedAtom {
            absolute_start: start,
            absolute_end,
            metric_after,
            special_after,
            kind,
        };
        self.emitted_offset = absolute_end;
        self.metric = metric_after;
        self.special = special_after;
        Ok(atom)
    }

    fn at_physical_eof(&self) -> bool {
        self.replay.is_none()
            && self.cursor.offset() == self.descriptor.bytes
            && self.read_offset == u64::try_from(self.descriptor.bytes).unwrap_or(u64::MAX)
    }
}

fn atom_metric(kind: CandidateSourceAtomKind) -> Result<(u64, u64), SourceBoundLedgerError> {
    match kind {
        CandidateSourceAtomKind::Scalar(scalar) => Ok((
            u64::try_from(scalar.len_utf8())
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
            u64::try_from(scalar.len_utf16())
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
        )),
        CandidateSourceAtomKind::Tab | CandidateSourceAtomKind::Nul => Ok((1, 1)),
        CandidateSourceAtomKind::LineEnding(ending) => Ok((ending.bytes(), ending.bytes())),
    }
}

fn increment_special(
    mut special: SpecialCounts,
    kind: CandidateSourceAtomKind,
) -> Result<SpecialCounts, SourceBoundLedgerError> {
    match kind {
        CandidateSourceAtomKind::Tab => {
            special.tabs = special
                .tabs
                .checked_add(1)
                .ok_or(SourceBoundLedgerError::Overflow("tab count"))?;
        }
        CandidateSourceAtomKind::Nul => {
            special.nuls = special
                .nuls
                .checked_add(1)
                .ok_or(SourceBoundLedgerError::Overflow("NUL count"))?;
        }
        CandidateSourceAtomKind::LineEnding(_) => {
            special.line_endings = special
                .line_endings
                .checked_add(1)
                .ok_or(SourceBoundLedgerError::Overflow("line-ending count"))?;
        }
        CandidateSourceAtomKind::Scalar(_) => {}
    }
    Ok(special)
}

#[derive(Clone, Copy, Debug)]
struct LineState {
    ordinal: u64,
    start: u64,
    metric_at_start: SourceLedgerMetric,
    has_atoms: bool,
    /// Absolute end of the last atom that was not space, tab, or a line
    /// ending. This lets the source authority certify a parser-selected blank
    /// suffix after already-claimed container markers without rescanning text.
    last_nonblank_end: u64,
    ending_atom: Option<CertifiedAtom>,
    eof: bool,
    atom_count: u64,
    atom_debug_digest: u64,
}

impl LineState {
    const fn new(ordinal: u64, start: u64, metric_at_start: SourceLedgerMetric) -> Self {
        Self {
            ordinal,
            start,
            metric_at_start,
            has_atoms: false,
            last_nonblank_end: start,
            ending_atom: None,
            eof: false,
            atom_count: 0,
            atom_debug_digest: FNV_OFFSET_BASIS,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct RecognitionLineState {
    ordinal: u64,
    start: u64,
    metric_at_start: SourceLedgerMetric,
    has_atoms: bool,
    ending: Option<CandidateLineEnding>,
    eof: bool,
    atom_count: u64,
    atom_debug_digest: u64,
}

#[derive(Clone, Copy, Debug)]
struct RecognitionByteSessionState {
    identity: CandidateRecognitionByteSession,
    exposed_high_water: usize,
    last_byte: Option<u8>,
    total_access_work_units: u64,
    new_bytes: u64,
    source_bytes_read: u64,
    repeated_last_byte_peeks: u64,
    decoded_atoms: u64,
    failed: bool,
}

impl RecognitionByteSessionState {
    const fn new(identity: CandidateRecognitionByteSession) -> Self {
        Self {
            identity,
            exposed_high_water: 0,
            last_byte: None,
            total_access_work_units: 0,
            new_bytes: 0,
            source_bytes_read: 0,
            repeated_last_byte_peeks: 0,
            decoded_atoms: 0,
            failed: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct RecognitionByteAccess {
    byte: u8,
    new_byte: bool,
    source_bytes_read: usize,
    decoded_atoms: usize,
}

impl RecognitionLineState {
    const fn new(ordinal: u64, start: u64, metric_at_start: SourceLedgerMetric) -> Self {
        Self {
            ordinal,
            start,
            metric_at_start,
            has_atoms: false,
            ending: None,
            eof: false,
            atom_count: 0,
            atom_debug_digest: FNV_OFFSET_BASIS,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct OpenRecognitionRange {
    kind: CandidateRecognitionRangeKind,
    first_line: u64,
    absolute_start: u64,
    metric_at_start: SourceLedgerMetric,
    line_count: u64,
    atom_count: u64,
    atom_debug_digest: u64,
}

#[derive(Clone, Copy, Debug)]
struct ReplayRangeProgress {
    metric_at_start: SourceLedgerMetric,
    line_count: u64,
    atom_count: u64,
    atom_debug_digest: u64,
}

#[derive(Clone, Copy, Debug)]
enum ExpectedReplay {
    Line(CandidateRecognitionLineReceipt),
    Range {
        receipt: CandidateRecognitionRangeReceipt,
        progress: ReplayRangeProgress,
    },
}

#[derive(Debug)]
struct RecognitionCursor {
    decoder: AtomDecoder,
    line: RecognitionLineState,
    expected_replay: Option<ExpectedReplay>,
    open_range: Option<OpenRecognitionRange>,
    byte_session: Option<RecognitionByteSessionState>,
    maximum_lead_bytes: u64,
}

impl RecognitionCursor {
    fn new(descriptor: SourceSnapshotDescriptor, cursor: CropSourceCursor) -> Self {
        Self {
            decoder: AtomDecoder::new(descriptor, cursor),
            line: RecognitionLineState::new(0, 0, SourceLedgerMetric::default()),
            expected_replay: None,
            open_range: None,
            byte_session: None,
            maximum_lead_bytes: 0,
        }
    }
}

/// One actor-poll borrow of the candidate-owned recognition byte session.
///
/// Only the last exposed byte is cached. The scanner retains its own logical
/// cursor; this source view retains only the sequential exposure high-water
/// needed to reject skips and rewinds beyond that single byte.
pub struct CandidateRecognitionByteSource<'a> {
    ledger: &'a mut CandidateSourceLedger,
    session: CandidateRecognitionByteSession,
    budget: usize,
    start_exposed_high_water: usize,
    access_work_units: usize,
    new_bytes: usize,
    source_bytes_read: usize,
    repeated_last_byte_peeks: usize,
    decoded_atoms: usize,
    budget_exhausted: bool,
    fatal: Option<SourceBoundLedgerError>,
}

impl CandidateRecognitionByteSource<'_> {
    #[must_use]
    pub const fn session(&self) -> CandidateRecognitionByteSession {
        self.session
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.session.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.session.is_empty()
    }

    /// Exact source-access budget still owned by this actor borrow.
    ///
    /// Grammar adapters may expose this value to a nested resumable scanner,
    /// but they cannot replenish it or retain the source view across polls.
    #[must_use]
    pub const fn remaining_access_budget(&self) -> usize {
        self.budget.saturating_sub(self.access_work_units)
    }

    /// Returns the next sequential local byte or the immediately preceding
    /// byte. The latter is the sole supported rewind and is served from one
    /// cached byte without re-feeding the decoder or metrics.
    pub fn byte_at(
        &mut self,
        local_offset: usize,
    ) -> Result<u8, CandidateRecognitionByteAccessError> {
        if self.fatal.is_some() {
            return Err(CandidateRecognitionByteAccessError::SessionFailed);
        }
        if self.access_work_units == self.budget {
            self.budget_exhausted = true;
            return Err(CandidateRecognitionByteAccessError::BudgetExhausted);
        }
        self.access_work_units += 1;
        match self
            .ledger
            .recognition_byte_access(self.session, local_offset)
        {
            Ok(access) => {
                if access.new_byte {
                    self.new_bytes += 1;
                } else {
                    self.repeated_last_byte_peeks += 1;
                }
                self.source_bytes_read += access.source_bytes_read;
                self.decoded_atoms += access.decoded_atoms;
                Ok(access.byte)
            }
            Err(error) => {
                self.fatal = Some(match error {
                    CandidateRecognitionByteAccessError::LogicalEof { .. } => {
                        SourceBoundLedgerError::RecognitionBytePastLine
                    }
                    CandidateRecognitionByteAccessError::OutOfOrder { .. } => {
                        SourceBoundLedgerError::RecognitionByteOutOfOrder
                    }
                    CandidateRecognitionByteAccessError::SessionFailed => {
                        SourceBoundLedgerError::RecognitionByteSessionFailed
                    }
                    CandidateRecognitionByteAccessError::Infrastructure(error) => error,
                    CandidateRecognitionByteAccessError::BudgetExhausted => {
                        unreachable!("the borrowed source owns budget exhaustion")
                    }
                });
                Err(error)
            }
        }
    }

    fn receipt(&self) -> Result<CandidateRecognitionBytePollReceipt, SourceBoundLedgerError> {
        let state = self
            .ledger
            .recognition
            .byte_session
            .as_ref()
            .ok_or(SourceBoundLedgerError::RecognitionByteSessionNotOpen)?;
        if state.identity != self.session {
            return Err(SourceBoundLedgerError::RecognitionByteWrongSession);
        }
        Ok(CandidateRecognitionBytePollReceipt {
            session: self.session,
            start_exposed_high_water: self.start_exposed_high_water,
            end_exposed_high_water: state.exposed_high_water,
            physical_high_water: self.ledger.recognition.decoder.read_offset,
            access_work_units: self.access_work_units,
            new_bytes: self.new_bytes,
            source_bytes_read: self.source_bytes_read,
            repeated_last_byte_peeks: self.repeated_last_byte_peeks,
            decoded_atoms: self.decoded_atoms,
            budget_exhausted: self.budget_exhausted,
            maximum_retained_byte_scratch: usize::from(state.last_byte.is_some()),
        })
    }
}

#[derive(Debug)]
struct PendingTerminator {
    debug_permit: Option<FreshCoveragePermit>,
    atom: CertifiedAtom,
    terminal: BindingStamp,
    affinity: GreenAffinity,
}

/// Read-only, writer-joined description of the one consumed but unresolved
/// physical line terminator. It carries no source-claim authority; its only
/// consumer is the active-Paragraph projection transaction, which rechecks
/// the same live binding before opening its cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CandidatePendingTerminator {
    source_start: SourceLedgerMetric,
    ending: CandidateLineEnding,
}

/// Atomic source-ledger result for a Paragraph erased by a reference-only
/// terminal. The parent binding is derived from the live path, and an
/// outstanding physical line ending (if any) is reminted as parent-owned
/// GAP/None only after the Paragraph has been retired.
pub(crate) struct CandidateReferenceOnlyRetirement {
    parent: CandidateOpenBinding,
    terminator_gap: Option<ConsumedSourcePiece>,
}

impl CandidateReferenceOnlyRetirement {
    pub(crate) fn into_parts(self) -> (CandidateOpenBinding, Option<ConsumedSourcePiece>) {
        (self.parent, self.terminator_gap)
    }
}

impl CandidatePendingTerminator {
    pub(crate) const fn source_start(self) -> SourceLedgerMetric {
        self.source_start
    }

    pub(crate) const fn ending(self) -> CandidateLineEnding {
        self.ending
    }
}

#[derive(Debug)]
struct PendingGap {
    debug_permit: Option<FreshCoveragePermit>,
    start: u64,
    end: u64,
    metric: SourceLedgerMetric,
    affinity: GreenAffinity,
    first_line: u64,
    binding_generation_ceiling: u64,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // Production-shaped proof state; not yet in the composite checkpoint.
struct DecoderLineBoundaryState {
    cursor_metrics: CursorMetrics,
    maximum_decoder_bytes: usize,
    eof_observed: bool,
}

/// Borrowed, non-authoritative shape of one deferred source predecessor at a
/// captured line boundary. This is pairing input for the same-build composite
/// checkpoint only; it cannot resolve or recreate the pending source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateLineBoundaryDeferredRole {
    None,
    Terminator { owner_depth: usize },
    BlankGap,
}

/// Zero-copy observation used to pair the source continuation with the real
/// parser pause and the writer's still-open bindings.
///
/// The continuation remains the sole resume capability. In particular, the
/// `BlockId`s and metrics observed through this view must never become a
/// cross-build constructor.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CandidateSourceLineBoundaryPairingView<'a> {
    continuation: &'a CandidateSourceLineBoundaryContinuation,
}

impl<'a> CandidateSourceLineBoundaryPairingView<'a> {
    pub(crate) const fn epoch(self) -> LiveCandidateEpoch {
        self.continuation.epoch
    }

    pub(crate) const fn line_ordinal(self) -> u64 {
        self.continuation.line_ordinal
    }

    pub(crate) const fn last_line_length(self) -> usize {
        self.continuation.last_line_length
    }

    pub(crate) const fn absolute_offset(self) -> u64 {
        self.continuation.absolute_offset
    }

    pub(crate) const fn emitted_metric(self) -> SourceLedgerMetric {
        self.continuation.metric
    }

    /// Physical source already accepted into projection/green. A pending
    /// terminator or blank gap has been decoded and advances `emitted_metric`,
    /// but remains deliberately outside this prefix until parser resolution.
    pub(crate) fn accepted_projection_metric(
        self,
    ) -> Result<SourceLedgerMetric, SourceBoundLedgerError> {
        if let Some(terminator) = &self.continuation.pending_terminator {
            let CandidateSourceAtomKind::LineEnding(ending) = terminator.atom.kind else {
                return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
            };
            let width = SourceLedgerMetric {
                bytes: ending.bytes(),
                utf16: ending.bytes(),
            };
            return self.continuation.metric.checked_sub(width);
        }
        if let Some(gap) = &self.continuation.pending_gap {
            return self.continuation.metric.checked_sub(gap.metric);
        }
        Ok(self.continuation.metric)
    }

    pub(crate) fn path_len(self) -> usize {
        self.continuation.path.len()
    }

    pub(crate) fn path_kinds(
        self,
    ) -> impl ExactSizeIterator<Item = GreenKind> + DoubleEndedIterator + 'a {
        self.continuation.path.iter().map(|stamp| stamp.kind)
    }

    pub(crate) fn path_logical_metric(self, index: usize) -> Option<SourceLedgerMetric> {
        self.continuation.path_logical_metrics.get(index).copied()
    }

    pub(crate) fn binding_matches(self, index: usize, binding: &CandidateOpenBinding) -> bool {
        self.continuation.path.get(index) == Some(&binding.stamp)
    }

    pub(crate) const fn structural_state_generation(self) -> u64 {
        self.continuation.structural_state_generation
    }

    pub(crate) const fn replayed_source_piece_count(self) -> u64 {
        self.continuation.claim_count
    }

    pub(crate) fn deferred_role(self) -> CandidateLineBoundaryDeferredRole {
        match (
            &self.continuation.pending_terminator,
            &self.continuation.pending_gap,
        ) {
            (None, None) => CandidateLineBoundaryDeferredRole::None,
            (Some(terminator), None) => CandidateLineBoundaryDeferredRole::Terminator {
                owner_depth: usize::try_from(terminator.terminal.depth).unwrap_or(usize::MAX),
            },
            (None, Some(_)) => CandidateLineBoundaryDeferredRole::BlankGap,
            (Some(_), Some(_)) => {
                debug_assert!(false, "captured continuation has two deferred predecessors");
                CandidateLineBoundaryDeferredRole::None
            }
        }
    }

    /// Checks the parser's optional marked-container blank floor without
    /// turning that observation into source ownership. The ledger's original
    /// binding-generation ceiling remains the actual stale-owner constraint.
    pub(crate) fn accepts_blank_gap_floor(self, floor: Option<usize>) -> bool {
        let Some(gap) = &self.continuation.pending_gap else {
            return false;
        };
        let Some(depth) = floor else {
            return true;
        };
        self.continuation.path.get(depth).is_some_and(|stamp| {
            stamp.path_generation < gap.binding_generation_ceiling
                && matches!(stamp.kind, GreenKind::BLOCK_QUOTE | GreenKind::ITEM)
        })
    }
}

/// Linear, same-build source-ledger control captured after one physical line
/// has been acknowledged by both source roles.
///
/// This is deliberately crate-private, non-cloneable, and non-serializable.
/// It contains build-scoped binding stamps and may own an unspent debug
/// coverage permit, so it is not a committed/cross-build checkpoint format.
/// Its only variable-size state is one stamp and one logical metric per open
/// binding. It retains no Crop cursor, source byte, UTF-8 decoder scratch,
/// recognition replay, range recipe, or source chunk buffer.
///
/// A durable checkpoint still needs the composite writer/green manifest to
/// re-authorize semantic identities and provisional normalization state in a
/// later build. This capability proves only the smaller same-build ledger
/// restart seam.
#[must_use = "a captured source continuation must be resumed or its candidate aborted"]
#[derive(Debug)]
#[allow(dead_code)] // Production-shaped proof state; not yet in the composite checkpoint.
pub(crate) struct CandidateSourceLineBoundaryContinuation {
    epoch: LiveCandidateEpoch,
    descriptor: SourceSnapshotDescriptor,
    authoritative_root_utf16: u64,
    absolute_offset: u64,
    line_ordinal: u64,
    last_line_length: usize,
    metric: SourceLedgerMetric,
    special: SpecialCounts,
    pending_terminator: Option<PendingTerminator>,
    pending_gap: Option<PendingGap>,
    path: Box<[BindingStamp]>,
    path_logical_metrics: Box<[SourceLedgerMetric]>,
    next_path_generation: u64,
    structural_state_generation: u64,
    maximum_open_path_len: usize,
    maximum_open_path_capacity: usize,
    line_count: u64,
    claim_count: u64,
    debug_digest: u64,
    authoritative_decoder: DecoderLineBoundaryState,
    recognition_decoder: DecoderLineBoundaryState,
    recognition_maximum_lead_bytes: u64,
    eof_observed: bool,
}

/// Source-ledger portion of the first in-memory cross-build Setext sample.
///
/// This is deliberately not the same-build continuation serialized by value:
/// build stamps, path generations, decoder receipts, debug permits, claim
/// counts, and digests are absent. Stable semantic identities and cumulative
/// source/path metrics survive only because construction validates the live
/// joined continuation's exact `Document -> Paragraph + pending LF` shape.
#[must_use = "a retained Setext source draft must be joined into a fresh candidate or discarded"]
#[derive(Debug)]
pub(crate) struct RetainedSetextSourceLedgerDraft {
    old_epoch: LiveCandidateEpoch,
    descriptor: SourceSnapshotDescriptor,
    physical_metric: SourceLedgerMetric,
    accepted_metric: SourceLedgerMetric,
    special: SpecialCounts,
    line_ordinal: u64,
    last_line_length: usize,
    path: Box<[RetainedSetextPathEntry]>,
    path_logical_metrics: Box<[SourceLedgerMetric]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RetainedSetextPathEntry {
    block: BlockId,
    kind: GreenKind,
}

impl RetainedSetextSourceLedgerDraft {
    pub(crate) const fn old_epoch(&self) -> LiveCandidateEpoch {
        self.old_epoch
    }

    pub(crate) const fn descriptor(&self) -> SourceSnapshotDescriptor {
        self.descriptor
    }

    pub(crate) const fn accepted_bytes(&self) -> u64 {
        self.accepted_metric.bytes
    }

    pub(crate) const fn accepted_utf16(&self) -> u64 {
        self.accepted_metric.utf16
    }

    pub(crate) const fn physical_bytes(&self) -> u64 {
        self.physical_metric.bytes
    }

    pub(crate) const fn physical_utf16(&self) -> u64 {
        self.physical_metric.utf16
    }

    pub(crate) const fn line_ordinal(&self) -> u64 {
        self.line_ordinal
    }

    pub(crate) fn last_line_length(&self) -> Result<u64, SourceBoundLedgerError> {
        u64::try_from(self.last_line_length)
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn retained_block_ids(&self) -> impl ExactSizeIterator<Item = BlockId> + '_ {
        self.path.iter().map(|entry| entry.block)
    }

    pub(crate) fn terminal_block(&self) -> Result<BlockId, SourceBoundLedgerError> {
        self.path
            .last()
            .map(|entry| entry.block)
            .ok_or(SourceBoundLedgerError::BindingNotOpen)
    }

    #[cfg(test)]
    #[allow(clippy::unused_self)] // Receipt is intentionally queried from the exact captured capability.
    pub(crate) const fn retained_source_bytes_for_test(&self) -> usize {
        0
    }
}

/// Fresh-build ledger plus the only bindings that match its reminted path.
/// The bindings are constructed inside the ledger firewall; no raw `BlockId` or
/// caller-provided stamp can enter this value.
#[derive(Debug)]
pub(crate) struct RestoredSetextSourceLedger {
    ledger: CandidateSourceLedger,
    bindings: Vec<CandidateOpenBinding>,
    #[cfg(feature = "exact-parser")]
    donor_cursor: DirectLineBoundaryResumeCursor,
}

/// Fresh-build ledger reconstructed exclusively from persisted parent
/// authority, unchanged-prefix lineage, the current SourceStore, and the one
/// normalized `CurrentRestartPath`.
///
/// Unlike [`RestoredSetextSourceLedger`], this carrier owns no transient old
/// writer/source draft. The path is kept linear beside the ledger and returned
/// at the downstream join so donor reconstruction and source bindings cannot
/// independently select divergent green paths.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct RestoredPersistedSourceLedger {
    parent_selection: RestartParentSelectionStamp,
    ledger: CandidateSourceLedger,
    bindings: Vec<CandidateOpenBinding>,
    donor_cursor: DirectLineBoundaryResumeCursor,
    path: crate::serialized_green::CurrentRestartPath,
    restart_anchor: crate::committed_checkpoint_index::ParentSelectedRestartAnchor,
    source_receipt: crate::retained_restart_coordinate::PersistedRestartSourceReceipt,
    receipt: PersistedSourceLedgerReconstructionReceipt,
}

/// Source ledger and parser resumed from one inseparable parent-selected
/// recipe/path pair. The raw donor recipe no longer exists once this value is
/// produced, so a same-cut recipe from another parent cannot be substituted at
/// a later layer.
#[cfg(feature = "exact-parser")]
pub(crate) struct DonorResumedPersistedSourceLedger {
    parent_selection: RestartParentSelectionStamp,
    ledger: CandidateSourceLedger,
    bindings: Vec<CandidateOpenBinding>,
    parser: DirectValueBlockParser,
    restart_anchor: crate::committed_checkpoint_index::ParentSelectedRestartAnchor,
    path: crate::serialized_green::CurrentRestartPath,
    source_receipt: crate::retained_restart_coordinate::PersistedRestartSourceReceipt,
    reconstruction_receipt: PersistedSourceLedgerReconstructionReceipt,
}

/// Persisted source/donor state after the exact parent selection has branded
/// the separately retained composite lease. The green restart role keeps the
/// optional Setext inverse inseparable from that branded lease; neither can be
/// routed through an equal-cut parent on its way to candidate installation.
#[cfg(feature = "exact-parser")]
#[must_use = "the parent-selected source activation must start green restart or be cancelled"]
pub(crate) struct ParentSelectedPersistedSourceActivation {
    ledger: CandidateSourceLedger,
    bindings: Vec<CandidateOpenBinding>,
    parser: DirectValueBlockParser,
    restart_anchor: crate::committed_checkpoint_index::ParentSelectedRestartAnchor,
    composer_coverage: ParentSelectedComposerCoverage,
    green: ParentSelectedPersistedGreenActivation,
    source_receipt: crate::retained_restart_coordinate::PersistedRestartSourceReceipt,
    reconstruction_receipt: PersistedSourceLedgerReconstructionReceipt,
}

#[cfg(feature = "exact-parser")]
#[must_use = "the branded parent green role must enter restart or be cancelled"]
pub(crate) enum ParentSelectedPersistedGreenActivation {
    Direct {
        lease: ParentSelectedRestartCompositeAdoptionLease,
        authority: crate::serialized_green::ParentSelectedDirectGreenRestartAuthority,
    },
    Setext {
        lease: ParentSelectedRestartCompositeAdoptionLease,
        inverse: crate::serialized_green::ParentSelectedSetextGreenInverseAuthority,
    },
}

/// Unforgeable lexical mint for the one source-module handoff into the
/// candidate writer. Another crate module may name the type required by the
/// writer constructor, but cannot construct a second handoff from independently
/// sourced ledger/green parts.
#[cfg(feature = "exact-parser")]
pub(crate) struct ParentSelectedCandidateWriterMint(());

/// Recoverable admission failure. Source authority is deliberately destroyed,
/// but the unbranded adoption lease is returned so the actor can resume and
/// abort the one fresh journal instead of leaking retained child owners.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct PersistedSourceAdoptionJoinFailure {
    pub(crate) error: PersistedSourceAdoptionJoinError,
    pub(crate) lease: RestartCompositeAdoptionLease,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) enum PersistedSourceAdoptionJoinError {
    Source(SourceBoundLedgerError),
    Path(crate::serialized_green::CurrentRestartPathError),
    Parent(RestartCompositeDocumentError),
}

/// Opaque cumulative composer base derived from the exact normalized green
/// path which was already bound to and consumed by donor resume.
///
/// No constructor accepts counters. The only mint is the atomic persisted
/// source/donor handoff below, after it rechecks the current ledger's A/P
/// deferred-LF state and every reminted open binding against that same path.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentSelectedComposerCoverage {
    epoch: LiveCandidateEpoch,
    accepted_source: SerializedMetric,
    event_cut: u64,
    projection_runs: u64,
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedComposerCoverage {
    pub(crate) const fn epoch(&self) -> LiveCandidateEpoch {
        self.epoch
    }

    pub(crate) const fn accepted_source(&self) -> SerializedMetric {
        self.accepted_source
    }

    pub(crate) const fn event_cut(&self) -> u64 {
        self.event_cut
    }

    pub(crate) const fn projection_runs(&self) -> u64 {
        self.projection_runs
    }

    #[cfg(test)]
    pub(crate) const fn mechanism_only_for_test(
        epoch: LiveCandidateEpoch,
        accepted_source: SerializedMetric,
        event_cut: u64,
        projection_runs: u64,
    ) -> Self {
        Self {
            epoch,
            accepted_source,
            event_cut,
            projection_runs,
        }
    }
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PersistedSourceDonorResumeError {
    DeferredRole(DirectLineBoundaryDeferredRole),
    Path(crate::serialized_green::CurrentRestartPathError),
    Parser(ParseError),
}

#[cfg(feature = "exact-parser")]
impl From<crate::serialized_green::CurrentRestartPathError> for PersistedSourceDonorResumeError {
    fn from(error: crate::serialized_green::CurrentRestartPathError) -> Self {
        Self::Path(error)
    }
}

#[cfg(feature = "exact-parser")]
impl From<ParseError> for PersistedSourceDonorResumeError {
    fn from(error: ParseError) -> Self {
        Self::Parser(error)
    }
}

#[cfg(feature = "exact-parser")]
fn require_persisted_lf_donor_role(
    grammar: &flark_comrak_value_block_core::DirectGrammarContinuation,
) -> Result<(), PersistedSourceDonorResumeError> {
    let deferred_role = grammar.deferred_role();
    if deferred_role == DirectLineBoundaryDeferredRole::Terminator {
        Ok(())
    } else {
        Err(PersistedSourceDonorResumeError::DeferredRole(deferred_role))
    }
}

#[cfg(feature = "exact-parser")]
fn require_persisted_source_complete_line_boundary_donor_role(
    grammar: &flark_comrak_value_block_core::DirectGrammarContinuation,
) -> Result<(), PersistedSourceDonorResumeError> {
    let deferred_role = grammar.deferred_role();
    if deferred_role == DirectLineBoundaryDeferredRole::None {
        Ok(())
    } else {
        Err(PersistedSourceDonorResumeError::DeferredRole(deferred_role))
    }
}

#[cfg(all(feature = "exact-parser", test))]
pub(crate) fn require_persisted_lf_donor_role_for_test(
    grammar: &flark_comrak_value_block_core::DirectGrammarContinuation,
) -> Result<(), PersistedSourceDonorResumeError> {
    require_persisted_lf_donor_role(grammar)
}

#[cfg(feature = "exact-parser")]
impl DonorResumedPersistedSourceLedger {
    #[must_use]
    pub(crate) const fn descriptor(&self) -> SourceSnapshotDescriptor {
        self.ledger.descriptor
    }

    #[must_use]
    pub(crate) fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    /// Read-only current-source progress for actor diagnostics. The resumed
    /// ledger, parser, bindings, parent stamp, and cursor authority remain
    /// inseparable inside this carrier.
    #[must_use]
    pub(crate) fn cursor_offset(&self) -> usize {
        self.ledger.cursor_offset()
    }

    #[must_use]
    pub(crate) const fn source_receipt(
        &self,
    ) -> crate::retained_restart_coordinate::PersistedRestartSourceReceipt {
        self.source_receipt
    }

    #[must_use]
    pub(crate) const fn reconstruction_receipt(
        &self,
    ) -> PersistedSourceLedgerReconstructionReceipt {
        self.reconstruction_receipt
    }

    /// Atomically brands the retained composite lease and destroys the last
    /// routable current-green path. All fallible source/path checks happen
    /// before branding; every failure therefore returns the still-unbranded
    /// lease for whole-journal cancellation.
    pub(crate) fn join_parent_adoption_lease(
        self,
        lease: RestartCompositeAdoptionLease,
    ) -> Result<ParentSelectedPersistedSourceActivation, PersistedSourceAdoptionJoinFailure> {
        let accepted_source = match self.validate_candidate_activation(&lease) {
            Ok(accepted) => accepted,
            Err(error) => {
                return Err(PersistedSourceAdoptionJoinFailure {
                    error: PersistedSourceAdoptionJoinError::Source(error),
                    lease,
                });
            }
        };
        let Self {
            parent_selection,
            ledger,
            bindings,
            parser,
            restart_anchor,
            path,
            source_receipt,
            reconstruction_receipt,
        } = self;
        let (_path_source, event_cut, projection_runs, green_authority) =
            match path.into_parent_selected_activation_parts() {
                Ok(parts) => parts,
                Err(error) => {
                    return Err(PersistedSourceAdoptionJoinFailure {
                        error: PersistedSourceAdoptionJoinError::Path(error),
                        lease,
                    });
                }
            };
        let (branded, restart_anchor) = match lease
            .join_parent_selection_and_restart_anchor(parent_selection, restart_anchor)
        {
            Ok(joined) => joined,
            Err(failure) => {
                return Err(PersistedSourceAdoptionJoinFailure {
                    error: PersistedSourceAdoptionJoinError::Parent(failure.error),
                    lease: failure.lease,
                });
            }
        };
        let green = match green_authority {
            crate::serialized_green::ParentSelectedGreenRestartAuthority::Direct(authority) => {
                ParentSelectedPersistedGreenActivation::Direct {
                    lease: branded,
                    authority,
                }
            }
            crate::serialized_green::ParentSelectedGreenRestartAuthority::Setext(inverse) => {
                ParentSelectedPersistedGreenActivation::Setext {
                    lease: branded,
                    inverse,
                }
            }
        };
        let composer_coverage = ParentSelectedComposerCoverage {
            epoch: ledger.epoch,
            accepted_source,
            event_cut,
            projection_runs,
        };
        Ok(ParentSelectedPersistedSourceActivation {
            ledger,
            bindings,
            parser,
            restart_anchor,
            composer_coverage,
            green,
            source_receipt,
            reconstruction_receipt,
        })
    }

    fn validate_candidate_activation(
        &self,
        lease: &RestartCompositeAdoptionLease,
    ) -> Result<SerializedMetric, SourceBoundLedgerError> {
        let accepted_source = match self.ledger.pending_terminator.as_ref() {
            Some(pending) => SerializedMetric {
                bytes: pending.atom.absolute_start,
                utf16: pending
                    .atom
                    .metric_after
                    .utf16
                    .checked_sub(1)
                    .ok_or(SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?,
            },
            None if self.ledger.pending_gap.is_none() => SerializedMetric {
                bytes: self.ledger.metric.bytes,
                utf16: self.ledger.metric.utf16,
            },
            None => {
                return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
            }
        };
        if self.ledger.descriptor != self.ledger.epoch.source()
            || lease.build_id() != self.ledger.epoch.build_id()
            || self.path.source_metric() != accepted_source
            || self.path.event_cut() == 0
            || self.path.coverage_count() == 0
            || self.path.open_depth()
                != u64::try_from(self.path.frames().len())
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            || self.path.frames().len() != self.bindings.len()
            || self
                .path
                .frames()
                .iter()
                .zip(&self.bindings)
                .any(|(frame, binding)| {
                    frame.block() != binding.block_id()
                        || persisted_donor_green_kind(frame).ok() != Some(binding.kind())
                })
        {
            return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
        }
        if let Some(pending) = self.ledger.pending_terminator.as_ref() {
            if pending.atom.kind != CandidateSourceAtomKind::LineEnding(CandidateLineEnding::Lf)
                || pending.atom.absolute_end
                    != pending.atom.absolute_start.checked_add(1).ok_or(
                        SourceBoundLedgerError::Overflow("persisted deferred LF byte cut"),
                    )?
                || pending.atom.metric_after.utf16
                    != accepted_source.utf16.checked_add(1).ok_or(
                        SourceBoundLedgerError::Overflow("persisted deferred LF UTF-16 cut"),
                    )?
                || self.bindings.last().map(|binding| binding.stamp) != Some(pending.terminal)
            {
                return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
            }
        }
        Ok(accepted_source)
    }

    /// Test-only receipt seam. Production activation must consume this bundle
    /// together with the exact branded parent-adoption lease. This extractor
    /// deliberately destroys the non-cloneable parent-selection stamp and is
    /// not the composition protocol.
    #[cfg(test)]
    pub(crate) fn into_candidate_parts_mechanism_only(
        self,
    ) -> (
        CandidateSourceLedger,
        Vec<CandidateOpenBinding>,
        DirectValueBlockParser,
        crate::serialized_green::CurrentRestartPath,
    ) {
        (self.ledger, self.bindings, self.parser, self.path)
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedPersistedSourceActivation {
    #[must_use]
    pub(crate) const fn descriptor(&self) -> SourceSnapshotDescriptor {
        self.ledger.descriptor
    }

    #[must_use]
    pub(crate) fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    #[must_use]
    pub(crate) const fn source_receipt(
        &self,
    ) -> crate::retained_restart_coordinate::PersistedRestartSourceReceipt {
        self.source_receipt
    }

    #[must_use]
    pub(crate) const fn reconstruction_receipt(
        &self,
    ) -> PersistedSourceLedgerReconstructionReceipt {
        self.reconstruction_receipt
    }

    /// Consumes the complete parent-selected source activation directly into
    /// the writer-owned retained-prefix state machine. This is deliberately a
    /// tailored handoff rather than an `into_parts` extractor: ledger,
    /// bindings, parser, cumulative composer coverage, and the branded green
    /// role cannot escape or be recombined between two equal-cut parents.
    pub(crate) fn try_begin_candidate_writer_restart(
        self,
        ticket: &crate::ArenaBuildTicket,
        arena: &crate::PageArena,
        config: crate::CandidateWriterConfig,
    ) -> Result<crate::ParentSelectedCandidateWriterRestart, crate::CandidateWriterError> {
        let Self {
            ledger,
            bindings,
            parser,
            restart_anchor,
            composer_coverage,
            green,
            source_receipt,
            reconstruction_receipt,
        } = self;
        let acknowledged_lines = ledger.line_count;
        crate::ParentSelectedCandidateWriterRestart::try_from_source_module_mint(
            ParentSelectedCandidateWriterMint(()),
            ledger,
            bindings,
            parser,
            acknowledged_lines,
            restart_anchor,
            composer_coverage,
            green,
            source_receipt,
            reconstruction_receipt,
            ticket,
            arena,
            config,
        )
    }

    pub(crate) fn cancel(
        self,
        session: crate::ArenaBuildSession<'_>,
    ) -> Result<ArenaBuildId, RestartCompositeDocumentError> {
        let Self {
            ledger: _,
            bindings: _,
            parser: _,
            restart_anchor: _,
            composer_coverage: _,
            green,
            source_receipt: _,
            reconstruction_receipt: _,
        } = self;
        green.cancel(session)
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedPersistedGreenActivation {
    #[must_use]
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        match self {
            Self::Direct { lease, .. } | Self::Setext { lease, .. } => lease.build_id(),
        }
    }

    pub(crate) fn cancel(
        self,
        session: crate::ArenaBuildSession<'_>,
    ) -> Result<ArenaBuildId, RestartCompositeDocumentError> {
        match self {
            Self::Direct { lease, .. } | Self::Setext { lease, .. } => lease.cancel(session),
        }
    }
}

/// Receipt for the storage-to-ledger reconstruction itself. SourceStore line
/// query work is reported by the coordinate receipt; this records that every
/// open binding/logical metric came from the single mapped path and that all
/// history-only counters start at a suffix-local baseline.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PersistedSourceLedgerReconstructionReceipt {
    pub path_frames_consumed: usize,
    pub logical_metrics_consumed: usize,
    pub suffix_local_tab_baseline: u64,
    pub suffix_local_nul_baseline: u64,
    pub suffix_local_line_ending_baseline: u64,
    pub suffix_local_claim_baseline: u64,
    pub retained_old_source_bytes: usize,
    pub retained_old_writer_drafts: usize,
    pub parallel_restart_paths: usize,
}

#[cfg(feature = "exact-parser")]
impl RestoredPersistedSourceLedger {
    #[must_use]
    pub(crate) const fn descriptor(&self) -> SourceSnapshotDescriptor {
        self.ledger.descriptor
    }

    #[must_use]
    pub(crate) const fn receipt(&self) -> PersistedSourceLedgerReconstructionReceipt {
        self.receipt
    }

    #[must_use]
    pub(crate) const fn source_receipt(
        &self,
    ) -> crate::retained_restart_coordinate::PersistedRestartSourceReceipt {
        self.source_receipt
    }

    pub(crate) fn accepted_projection_metric(
        &self,
    ) -> Result<SourceLedgerMetric, SourceBoundLedgerError> {
        match self.ledger.pending_terminator.as_ref() {
            Some(pending) => Ok(SourceLedgerMetric {
                bytes: pending.atom.absolute_start,
                utf16: pending
                    .atom
                    .metric_after
                    .utf16
                    .checked_sub(1)
                    .ok_or(SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?,
            }),
            None if self.ledger.pending_gap.is_none() => Ok(self.ledger.metric),
            None => Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable),
        }
    }

    #[must_use]
    pub(crate) fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    pub(crate) fn terminal_block(&self) -> Result<BlockId, SourceBoundLedgerError> {
        self.bindings
            .last()
            .map(CandidateOpenBinding::block_id)
            .ok_or(SourceBoundLedgerError::BindingNotOpen)
    }

    pub(crate) fn terminal_kind(&self) -> Result<GreenKind, SourceBoundLedgerError> {
        self.bindings
            .last()
            .map(CandidateOpenBinding::kind)
            .ok_or(SourceBoundLedgerError::BindingNotOpen)
    }

    #[must_use]
    pub(crate) const fn completed_line_ordinal(&self) -> u64 {
        self.ledger.line_count
    }

    /// Final atomic donor join. The exact selected recipe is decoded only here
    /// and immediately bound to the exact normalized path carried through the
    /// source-lineage job. Neither half is ever returned in a separately
    /// pairable pre-resume form.
    pub(crate) fn resume_parent_selected_donor(
        self,
    ) -> Result<DonorResumedPersistedSourceLedger, PersistedSourceDonorResumeError> {
        let (grammar, line_local) = self.restart_anchor.decode_grammar_parts()?;
        if self.ledger.pending_terminator.is_some() {
            require_persisted_lf_donor_role(&grammar)?;
        } else {
            require_persisted_source_complete_line_boundary_donor_role(&grammar)?;
        }
        let bound = self
            .path
            .bind_direct_restart_output_from_stabilized_line_mechanism_only(&grammar, line_local)?;
        let (path, output) = bound.into_parts();
        let parser =
            DirectValueBlockParser::resume_restart_parts(&grammar, output, self.donor_cursor)?;
        Ok(DonorResumedPersistedSourceLedger {
            parent_selection: self.parent_selection,
            ledger: self.ledger,
            bindings: self.bindings,
            parser,
            restart_anchor: self.restart_anchor,
            path,
            source_receipt: self.source_receipt,
            reconstruction_receipt: self.receipt,
        })
    }

    #[cfg(test)]
    pub(crate) fn suffix_local_receipt_for_test(
        &self,
    ) -> (SourceLedgerMetric, bool, u64, u64, SourceLedgerMetric) {
        (
            self.ledger.metric,
            self.ledger.special.is_zero(),
            self.ledger.claim_count,
            self.ledger.debug_digest,
            *self
                .ledger
                .path_logical_metrics
                .last()
                .expect("persisted restart has an open terminal frame"),
        )
    }
}

#[cfg(feature = "exact-parser")]
fn persisted_donor_green_kind(
    frame: &crate::serialized_green::CurrentRestartPathFrame,
) -> Result<GreenKind, SourceBoundLedgerError> {
    use crate::serialized_green::CurrentRestartNormalizationRole;

    let donor_kind = match frame.donor().kind {
        DirectBlockKind::Document => GreenKind::DOCUMENT,
        DirectBlockKind::BlockQuote => GreenKind::BLOCK_QUOTE,
        DirectBlockKind::List(_) => GreenKind::LIST,
        DirectBlockKind::Item(_) => GreenKind::ITEM,
        DirectBlockKind::Paragraph => GreenKind::PARAGRAPH,
        DirectBlockKind::Heading(_) => GreenKind::HEADING,
        DirectBlockKind::FencedCode(_) => GreenKind::FENCED_CODE,
    };
    match (frame.green_kind(), donor_kind, frame.normalization()) {
        (green, donor, None) if green == donor => Ok(donor),
        (GreenKind::HEADING, GreenKind::PARAGRAPH, Some(metadata))
            if metadata.role()
                == CurrentRestartNormalizationRole::SetextHeadingToProvisionalParagraph =>
        {
            Ok(GreenKind::PARAGRAPH)
        }
        _ => Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable),
    }
}

#[cfg(feature = "exact-parser")]
fn validate_persisted_lf_path_shape(
    path: &crate::serialized_green::CurrentRestartPath,
    coordinates: crate::retained_restart_coordinate::PersistedRestartCoordinateView,
) -> Result<(), SourceBoundLedgerError> {
    let frames = path.frames();
    if path.source_metric().bytes
        != u64::try_from(coordinates.old_accepted_projection_cut)
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
        || path.source_metric().utf16
            != u64::try_from(coordinates.old_accepted_projection_utf16)
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
        || frames.len() != 2
        || path.open_depth() != 2
        || frames.first().map(persisted_donor_green_kind).transpose()? != Some(GreenKind::DOCUMENT)
        || frames.last().map(persisted_donor_green_kind).transpose()? != Some(GreenKind::PARAGRAPH)
        || frames.last().map(|frame| frame.logical_metric())
            != Some(SerializedMetric {
                bytes: u64::try_from(coordinates.current_accepted_projection_cut)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
                utf16: u64::try_from(coordinates.current_accepted_projection_utf16)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
            })
    {
        return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
    }
    Ok(())
}

#[cfg(feature = "exact-parser")]
fn validate_persisted_source_complete_line_boundary_path_shape(
    path: &crate::serialized_green::CurrentRestartPath,
    coordinates: crate::retained_restart_coordinate::PersistedRestartCoordinateView,
) -> Result<(), SourceBoundLedgerError> {
    let frames = path.frames();
    if path.source_metric().bytes
        != u64::try_from(coordinates.old_physical_restart)
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
        || path.source_metric().utf16
            != u64::try_from(coordinates.old_physical_restart_utf16)
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
        || coordinates.old_accepted_projection_cut != coordinates.old_physical_restart
        || coordinates.old_accepted_projection_utf16 != coordinates.old_physical_restart_utf16
        || frames.is_empty()
        || path.open_depth()
            != u64::try_from(frames.len())
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
        || frames.first().map(persisted_donor_green_kind).transpose()? != Some(GreenKind::DOCUMENT)
        || frames.iter().any(|frame| {
            frame.normalization().is_some() || persisted_donor_green_kind(frame).is_err()
        })
    {
        return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
    }
    Ok(())
}

#[cfg(all(feature = "exact-parser", test))]
pub(crate) fn validate_persisted_lf_path_shape_for_test(
    path: &crate::serialized_green::CurrentRestartPath,
    coordinates: crate::retained_restart_coordinate::PersistedRestartCoordinateView,
) -> Result<(), SourceBoundLedgerError> {
    validate_persisted_lf_path_shape(path, coordinates)
}

impl RestoredSetextSourceLedger {
    pub(crate) const fn descriptor(&self) -> SourceSnapshotDescriptor {
        self.ledger.descriptor
    }

    pub(crate) fn accepted_projection_metric(
        &self,
    ) -> Result<SourceLedgerMetric, SourceBoundLedgerError> {
        let pending = self
            .ledger
            .pending_terminator
            .as_ref()
            .ok_or(SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?;
        let accepted = SourceLedgerMetric {
            bytes: pending.atom.absolute_start,
            utf16: pending
                .atom
                .metric_after
                .utf16
                .checked_sub(1)
                .ok_or(SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?,
        };
        if self.ledger.path_logical_metrics.last().copied() != Some(accepted) {
            return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
        }
        Ok(accepted)
    }

    pub(crate) fn terminal_block(&self) -> Result<BlockId, SourceBoundLedgerError> {
        self.bindings
            .last()
            .map(CandidateOpenBinding::block_id)
            .ok_or(SourceBoundLedgerError::BindingNotOpen)
    }

    pub(crate) fn terminal_kind(&self) -> Result<GreenKind, SourceBoundLedgerError> {
        self.bindings
            .last()
            .map(CandidateOpenBinding::kind)
            .ok_or(SourceBoundLedgerError::BindingNotOpen)
    }

    pub(crate) fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    pub(crate) const fn completed_line_ordinal(&self) -> u64 {
        self.ledger.line_count
    }

    #[cfg(feature = "exact-parser")]
    pub(crate) fn into_parts(
        self,
    ) -> (
        CandidateSourceLedger,
        Vec<CandidateOpenBinding>,
        DirectLineBoundaryResumeCursor,
    ) {
        (self.ledger, self.bindings, self.donor_cursor)
    }

    #[cfg(test)]
    pub(crate) fn suffix_local_receipt_for_test(
        &self,
    ) -> (SourceLedgerMetric, u64, u64, u64, SourceLedgerMetric) {
        (
            self.ledger.metric,
            self.ledger.line_count,
            self.ledger.claim_count,
            self.ledger.debug_digest,
            self.ledger.path_logical_metrics[1],
        )
    }
}

#[allow(dead_code)] // Production-shaped proof methods; tests exercise the pending integration.
impl CandidateSourceLineBoundaryContinuation {
    pub(crate) const fn absolute_offset(&self) -> u64 {
        self.absolute_offset
    }

    pub(crate) const fn pairing_view(&self) -> CandidateSourceLineBoundaryPairingView<'_> {
        CandidateSourceLineBoundaryPairingView { continuation: self }
    }

    /// Seals only the narrow production Setext shape selected for the first
    /// cross-build mechanism proof. The already-joined writer/parser boundary
    /// is the caller; coordinates are derived here and never supplied as
    /// scalar constructor arguments.
    pub(crate) fn seal_retained_setext_source_draft(
        &self,
    ) -> Result<RetainedSetextSourceLedgerDraft, SourceBoundLedgerError> {
        let pending = self
            .pending_terminator
            .as_ref()
            .ok_or(SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?;
        if self.pending_gap.is_some()
            || pending.debug_permit.is_some()
            || pending.affinity != GreenAffinity::Downstream
            || pending.atom.kind != CandidateSourceAtomKind::LineEnding(CandidateLineEnding::Lf)
            || self.path.len() != 2
            || self.path_logical_metrics.len() != 2
            || self.path[0].kind != GreenKind::DOCUMENT
            || self.path[1].kind != GreenKind::PARAGRAPH
            || pending.terminal != self.path[1]
            || self.path.iter().any(|stamp| stamp.block.0 == 0)
            || self.absolute_offset != self.metric.bytes
            || pending.atom.absolute_end != self.absolute_offset
            || pending.atom.absolute_start.checked_add(1) != Some(self.absolute_offset)
            || pending.atom.metric_after != self.metric
            || pending.atom.special_after != self.special
            || pending.atom.line_ordinal.checked_add(1) != Some(self.line_ordinal)
            || pending.atom.line_start.checked_add(
                u64::try_from(self.last_line_length)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
            ) != Some(pending.atom.absolute_start)
            || self.eof_observed
        {
            return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
        }
        let accepted_metric = self
            .metric
            .checked_sub(SourceLedgerMetric { bytes: 1, utf16: 1 })?;
        let mut path = Vec::new();
        path.try_reserve_exact(self.path.len())
            .map_err(|_| SourceBoundLedgerError::Invariant("retained Setext path allocation"))?;
        path.extend(self.path.iter().map(|stamp| RetainedSetextPathEntry {
            block: stamp.block,
            kind: stamp.kind,
        }));
        let mut logical = Vec::new();
        logical
            .try_reserve_exact(self.path_logical_metrics.len())
            .map_err(|_| SourceBoundLedgerError::Invariant("retained Setext metric allocation"))?;
        logical.extend_from_slice(&self.path_logical_metrics);
        Ok(RetainedSetextSourceLedgerDraft {
            old_epoch: self.epoch,
            descriptor: self.descriptor,
            physical_metric: self.metric,
            accepted_metric,
            special: self.special,
            line_ordinal: self.line_ordinal,
            last_line_length: self.last_line_length,
            path: path.into_boxed_slice(),
            path_logical_metrics: logical.into_boxed_slice(),
        })
    }

    /// Stops the actual candidate decoder at an exact physical line boundary
    /// by consuming source-lineage authority for the unchanged tail.
    ///
    /// This is a different terminal state from EOF replay. Prefix counters and
    /// decoder receipts stay explicitly prefix-only; final metric/line facts
    /// come from the storage-bound tail. The current open identities must be
    /// the same identities retained by the old green path, because adopted
    /// projection runs target relative open depths in packed storage. Before
    /// validation, the immutable old suffix is therefore rebound to the live
    /// identities at those exact depths; kinds and deferred-owner position
    /// must still agree.
    pub(crate) fn seal_adopted_tail(
        self,
        tail: SourceBoundGreenTailAdoption,
    ) -> Result<
        (
            CandidateAdoptedSourceSeal,
            GreenComposerTailAdoptionAuthority,
        ),
        SourceBoundLedgerError,
    > {
        self.validate_adopted_tail(&tail)?;
        let final_metric = tail.final_metric();
        let accepted = tail.current_prefix();
        let accepted_projection_prefix_metric = SourceLedgerMetric {
            bytes: accepted.bytes,
            utf16: accepted.utf16,
        };
        let physical_parser_prefix_metric = self.metric;
        let metric = SourceLedgerMetric {
            bytes: final_metric.bytes,
            utf16: final_metric.utf16,
        };
        let maximum_open_path_capacity_bytes = self
            .maximum_open_path_capacity
            .saturating_mul(std::mem::size_of::<BindingStamp>())
            .saturating_add(
                self.path_logical_metrics
                    .len()
                    .saturating_mul(std::mem::size_of::<SourceLedgerMetric>()),
            );
        let source = CandidateAdoptedSourceSeal {
            descriptor: self.descriptor,
            build: self.epoch.build_id(),
            accepted_projection_prefix_metric,
            physical_parser_prefix_metric,
            metric,
            replayed_prefix_line_count: self.line_count,
            line_count: tail.total_line_count(),
            replayed_source_piece_count: self.claim_count,
            prefix_debug_digest: self.debug_digest,
            maximum_decoder_bytes: self.authoritative_decoder.maximum_decoder_bytes,
            source_chunk_loads: self.authoritative_decoder.cursor_metrics.chunk_loads,
            source_bytes_copied: self.authoritative_decoder.cursor_metrics.chunk_bytes_copied,
            maximum_source_chunk_bytes: self
                .authoritative_decoder
                .cursor_metrics
                .maximum_chunk_bytes,
            recognition_source_chunk_loads: self.recognition_decoder.cursor_metrics.chunk_loads,
            recognition_source_bytes_copied: self
                .recognition_decoder
                .cursor_metrics
                .chunk_bytes_copied,
            recognition_maximum_source_chunk_bytes: self
                .recognition_decoder
                .cursor_metrics
                .maximum_chunk_bytes,
            recognition_maximum_decoder_bytes: self.recognition_decoder.maximum_decoder_bytes,
            recognition_maximum_lead_bytes: self.recognition_maximum_lead_bytes,
            authoritative_root_utf16: self.authoritative_root_utf16,
            maximum_open_path_len: self.maximum_open_path_len.max(self.path.len()),
            maximum_open_path_capacity_bytes,
        };
        Ok((source, tail.into_composer_authority()))
    }

    pub(crate) fn validate_adopted_tail(
        &self,
        tail: &SourceBoundGreenTailAdoption,
    ) -> Result<(), SourceBoundLedgerError> {
        let accepted = tail.current_prefix();
        let physical = tail.physical_prefix();
        let final_metric = tail.final_metric();
        let frames = tail.open_frames();
        self.validate_deferred_tail_predecessor(tail, accepted, physical)?;
        if self.epoch != tail.epoch()
            || self.descriptor != tail.epoch().source()
            || self.epoch.build_id() != tail.epoch().build_id()
            || self.pending_gap.is_some()
            || self.eof_observed
            || self.authoritative_decoder.eof_observed
            || self.recognition_decoder.eof_observed
            || self.absolute_offset != self.metric.bytes
            || self.metric.bytes != physical.bytes
            || self.metric.utf16 != physical.utf16
            || self.line_ordinal != tail.current_line_ordinal()
            || self.line_count != self.line_ordinal
            || self.path.len() != self.path_logical_metrics.len()
            || self.path.len() != frames.len()
            || self.authoritative_root_utf16 != final_metric.utf16
            || final_metric.bytes
                != u64::try_from(self.descriptor.bytes)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            || tail.total_line_count() < self.line_count
        {
            return Err(SourceBoundLedgerError::TailAdoptionMismatch);
        }
        for (depth, (stamp, frame)) in self.path.iter().zip(frames).enumerate() {
            if stamp.build != self.epoch.build_id()
                || usize::try_from(stamp.depth)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
                    != depth
                || Some(stamp.block) != tail.current_open_block(depth)
                || stamp.kind != frame.kind()
            {
                return Err(SourceBoundLedgerError::TailAdoptionMismatch);
            }
        }
        Ok(())
    }

    fn validate_deferred_tail_predecessor(
        &self,
        tail: &SourceBoundGreenTailAdoption,
        accepted: SerializedMetric,
        physical: SerializedMetric,
    ) -> Result<(), SourceBoundLedgerError> {
        match (tail.deferred_terminator(), &self.pending_terminator) {
            (None, None) if accepted == physical => Ok(()),
            (Some(authority), Some(pending)) => {
                let expected = match authority.ending() {
                    GreenDeferredLineEnding::Lf => CandidateLineEnding::Lf,
                    GreenDeferredLineEnding::LoneCr => CandidateLineEnding::LoneCr,
                    GreenDeferredLineEnding::CrLf => CandidateLineEnding::CrLf,
                };
                let width = SourceLedgerMetric {
                    bytes: expected.bytes(),
                    utf16: expected.bytes(),
                };
                let accepted_metric = pending.atom.metric_after.checked_sub(width)?;
                if pending.atom.kind != CandidateSourceAtomKind::LineEnding(expected)
                    || pending.atom.absolute_start != accepted.bytes
                    || pending.atom.absolute_end != physical.bytes
                    || pending.atom.metric_after.bytes != physical.bytes
                    || pending.atom.metric_after.utf16 != physical.utf16
                    || accepted_metric.bytes != accepted.bytes
                    || accepted_metric.utf16 != accepted.utf16
                    || Some(pending.terminal.block) != tail.current_deferred_owner()
                    || pending.terminal.kind != authority.owner_kind()
                    || pending.affinity != GreenAffinity::Downstream
                    || !matches!(
                        authority.part(),
                        CoveragePart::CONTENT | CoveragePart::TERMINAL
                    )
                {
                    return Err(SourceBoundLedgerError::TailAdoptionMismatch);
                }
                Ok(())
            }
            _ => Err(SourceBoundLedgerError::TailAdoptionMismatch),
        }
    }

    /// Binds relative-depth green coverage to this exact candidate path. Only
    /// the source ledger can mint the authority consumed by the tail object.
    pub(crate) fn rebind_adopted_tail(
        &self,
        tail: &mut SourceBoundGreenTailAdoption,
    ) -> Result<(), SourceBoundLedgerError> {
        tail.rebind_current_open_path(
            GreenTailOpenPathRebindMint(()),
            self.path.iter().map(|stamp| (stamp.block, stamp.kind)),
        )
        .map_err(|_| SourceBoundLedgerError::TailAdoptionMismatch)
    }

    pub(crate) fn validate_resume_authority(
        &self,
        epoch: LiveCandidateEpoch,
        authoritative_root_utf16: usize,
        authoritative: &CropSourceCursor,
        recognition: &CropSourceCursor,
        physical_line_start: bool,
    ) -> Result<(), SourceBoundLedgerError> {
        if epoch != self.epoch
            || epoch.source() != self.descriptor
            || epoch.build_id() != self.epoch.build_id()
        {
            return Err(SourceBoundLedgerError::WrongEpoch);
        }
        if authoritative.source_identity() != self.descriptor.root
            || recognition.source_identity() != self.descriptor.root
        {
            return Err(SourceBoundLedgerError::WrongSourceRoot);
        }
        let offset = usize::try_from(self.absolute_offset)
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?;
        if authoritative.offset() != offset || recognition.offset() != offset {
            return Err(SourceBoundLedgerError::WrongSourceOffset);
        }
        if !physical_line_start && !self.eof_observed {
            return Err(SourceBoundLedgerError::ResumeOffsetIsNotPhysicalLineStart);
        }
        if u64::try_from(authoritative_root_utf16)
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            != self.authoritative_root_utf16
        {
            return Err(SourceBoundLedgerError::RootUtf16Mismatch);
        }
        Ok(())
    }

    /// Installs already actor-validated cursors. Callers must first use
    /// `validate_resume_authority`; keeping this step infallible lets the actor
    /// validate before consuming the linear continuation.
    pub(crate) fn resume_with_validated_cursors(
        self,
        authoritative: CropSourceCursor,
        recognition: CropSourceCursor,
    ) -> CandidateSourceLedger {
        let authoritative_decoder = AtomDecoder::from_emitted_boundary(
            self.descriptor,
            authoritative,
            self.absolute_offset,
            self.metric,
            self.special,
            self.authoritative_decoder,
        );
        let recognition_decoder = AtomDecoder::from_emitted_boundary(
            self.descriptor,
            recognition,
            self.absolute_offset,
            self.metric,
            self.special,
            self.recognition_decoder,
        );
        CandidateSourceLedger {
            epoch: self.epoch,
            descriptor: self.descriptor,
            authoritative_root_utf16: self.authoritative_root_utf16,
            authoritative: authoritative_decoder,
            recognition: RecognitionCursor {
                decoder: recognition_decoder,
                line: RecognitionLineState::new(
                    self.line_ordinal,
                    self.absolute_offset,
                    self.metric,
                ),
                expected_replay: None,
                open_range: None,
                byte_session: None,
                maximum_lead_bytes: self.recognition_maximum_lead_bytes,
            },
            metric: self.metric,
            special: self.special,
            line: LineState::new(self.line_ordinal, self.absolute_offset, self.metric),
            next_claim_offset: self.absolute_offset,
            next_claim_metric: self.metric,
            next_claim_special: self.special,
            pending_terminator: self.pending_terminator,
            pending_gap: self.pending_gap,
            path: self.path.into_vec(),
            path_logical_metrics: self.path_logical_metrics.into_vec(),
            next_path_generation: self.next_path_generation,
            structural_state_generation: self.structural_state_generation,
            maximum_open_path_len: self.maximum_open_path_len,
            maximum_open_path_capacity: self.maximum_open_path_capacity,
            last_line_length: self.last_line_length,
            line_count: self.line_count,
            claim_count: self.claim_count,
            debug_digest: self.debug_digest,
            eof_observed: self.eof_observed,
            sealed: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn retained_heap_bytes_for_test(&self) -> usize {
        self.path
            .len()
            .saturating_mul(std::mem::size_of::<BindingStamp>())
            .saturating_add(
                self.path_logical_metrics
                    .len()
                    .saturating_mul(std::mem::size_of::<SourceLedgerMetric>()),
            )
    }

    #[cfg(test)]
    #[allow(clippy::unused_self)] // The type-level receipt states that no field owns source.
    pub(crate) const fn retained_source_bytes_for_test(&self) -> usize {
        0
    }
}

/// Candidate-owned streaming ledger. Construction is crate-private so only
/// `LiveDocumentStore` can pair it with the real source lease and build epoch.
#[derive(Debug)]
pub(crate) struct CandidateSourceLedger {
    epoch: LiveCandidateEpoch,
    descriptor: SourceSnapshotDescriptor,
    authoritative_root_utf16: u64,
    authoritative: AtomDecoder,
    recognition: RecognitionCursor,
    metric: SourceLedgerMetric,
    special: SpecialCounts,
    line: LineState,
    next_claim_offset: u64,
    next_claim_metric: SourceLedgerMetric,
    next_claim_special: SpecialCounts,
    pending_terminator: Option<PendingTerminator>,
    pending_gap: Option<PendingGap>,
    path: Vec<BindingStamp>,
    path_logical_metrics: Vec<SourceLedgerMetric>,
    next_path_generation: u64,
    structural_state_generation: u64,
    maximum_open_path_len: usize,
    maximum_open_path_capacity: usize,
    last_line_length: usize,
    line_count: u64,
    claim_count: u64,
    debug_digest: u64,
    eof_observed: bool,
    sealed: bool,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PersistedSourceRestartShape {
    DeferredLf,
    SourceCompleteLineBoundary,
}

#[cfg(feature = "exact-parser")]
struct PersistedSourceRestartParts {
    current_source: SourceSnapshotDescriptor,
    coordinates: crate::retained_restart_coordinate::PersistedRestartCoordinateView,
    previous: crate::SourcePhysicalLinePredecessor,
    cursors: crate::SourceResumeCursorPair,
    path: crate::serialized_green::CurrentRestartPath,
    restart_anchor: crate::committed_checkpoint_index::ParentSelectedRestartAnchor,
    parent_selection: crate::storage_only_composite_document::RestartParentSelectionStamp,
    source_receipt: crate::retained_restart_coordinate::PersistedRestartSourceReceipt,
}

impl CandidateSourceLedger {
    pub(crate) fn new(
        epoch: LiveCandidateEpoch,
        descriptor: SourceSnapshotDescriptor,
        authoritative_root_utf16: usize,
        cursor: CropSourceCursor,
        recognition_cursor: CropSourceCursor,
    ) -> Self {
        debug_assert_eq!(epoch.source(), descriptor);
        debug_assert_eq!(epoch.source().root, cursor.source_identity());
        Self {
            epoch,
            descriptor,
            authoritative_root_utf16: u64::try_from(authoritative_root_utf16)
                .expect("Crop UTF-16 length fits u64"),
            authoritative: AtomDecoder::new(descriptor, cursor),
            recognition: RecognitionCursor::new(descriptor, recognition_cursor),
            metric: SourceLedgerMetric::default(),
            special: SpecialCounts::default(),
            line: LineState::new(0, 0, SourceLedgerMetric::default()),
            next_claim_offset: 0,
            next_claim_metric: SourceLedgerMetric::default(),
            next_claim_special: SpecialCounts::default(),
            pending_terminator: None,
            pending_gap: None,
            path: Vec::new(),
            path_logical_metrics: Vec::new(),
            next_path_generation: 1,
            structural_state_generation: 1,
            maximum_open_path_len: 0,
            maximum_open_path_capacity: 0,
            last_line_length: 0,
            line_count: 0,
            claim_count: 0,
            debug_digest: FNV_OFFSET_BASIS,
            eof_observed: false,
            sealed: false,
        }
    }

    /// Installs a fresh-build ledger at the retained Setext seam. Only the
    /// sealed old source draft and the consuming lineage authority can reach
    /// this constructor; callers cannot supply a `BlockId`, stamp, physical
    /// metric, or accepted metric independently.
    #[allow(clippy::too_many_lines)] // The firewall validates and remints every ledger field in one consuming transition.
    pub(crate) fn restore_retained_setext(
        epoch: LiveCandidateEpoch,
        draft: RetainedSetextSourceLedgerDraft,
        preferred: crate::retained_restart_coordinate::PreferredDeferredLfRestart,
    ) -> Result<RestoredSetextSourceLedger, SourceBoundLedgerError> {
        let coordinates = preferred.coordinates();
        if draft.old_epoch.source() != draft.descriptor
            || preferred.from() != draft.descriptor
            || preferred.to() != epoch.source()
            || coordinates.old_accepted_projection_cut
                != usize::try_from(draft.accepted_metric.bytes)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            || coordinates.old_accepted_projection_utf16
                != usize::try_from(draft.accepted_metric.utf16)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            || coordinates.old_physical_restart
                != usize::try_from(draft.physical_metric.bytes)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            || coordinates.old_physical_restart_utf16
                != usize::try_from(draft.physical_metric.utf16)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            || coordinates.old_completed_line_ordinal != draft.line_ordinal
            || coordinates.old_previous_content_bytes
                != u64::try_from(draft.last_line_length)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            || coordinates.current_completed_line_ordinal != draft.line_ordinal
            || coordinates.current_previous_content_bytes
                != u64::try_from(draft.last_line_length)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            || coordinates.current_accepted_projection_cut.checked_add(1)
                != Some(coordinates.current_physical_restart)
            || coordinates.current_accepted_projection_utf16.checked_add(1)
                != Some(coordinates.current_physical_restart_utf16)
            || draft.path.len() != 2
            || draft.path_logical_metrics.len() != 2
            || draft.path[0].kind != GreenKind::DOCUMENT
            || draft.path[1].kind != GreenKind::PARAGRAPH
        {
            return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
        }

        #[cfg(feature = "exact-parser")]
        let donor_cursor = DirectLineBoundaryResumeCursor::new(
            coordinates.current_completed_line_ordinal,
            coordinates.current_previous_content_bytes,
        )
        .map_err(|_| SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?;

        let pair = preferred.into_cursor_pair();
        if pair.descriptor() != epoch.source()
            || pair.offset() != coordinates.current_physical_restart
            || !pair.is_physical_line_start()
        {
            return Err(SourceBoundLedgerError::WrongSourceOffset);
        }
        let authoritative_root_utf16 = u64::try_from(pair.total_utf16())
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?;
        let (authoritative, recognition) = pair.into_cursors();
        let physical_metric = SourceLedgerMetric {
            bytes: u64::try_from(coordinates.current_physical_restart)
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
            utf16: u64::try_from(coordinates.current_physical_restart_utf16)
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
        };
        let accepted_start = u64::try_from(coordinates.current_accepted_projection_cut)
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?;
        let line_start = accepted_start
            .checked_sub(
                u64::try_from(draft.last_line_length)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
            )
            .ok_or(SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?;

        let mut path = Vec::new();
        path.try_reserve_exact(draft.path.len())
            .map_err(|_| SourceBoundLedgerError::Invariant("restored Setext path allocation"))?;
        let mut bindings = Vec::new();
        bindings
            .try_reserve_exact(draft.path.len())
            .map_err(|_| SourceBoundLedgerError::Invariant("restored Setext binding allocation"))?;
        for (depth, retained) in draft.path.iter().copied().enumerate() {
            let path_generation = u64::try_from(depth)
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
                .checked_add(1)
                .ok_or(SourceBoundLedgerError::Overflow("restored path generation"))?;
            let stamp = BindingStamp {
                build: epoch.build_id(),
                block: retained.block,
                kind: retained.kind,
                depth: u32::try_from(depth)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
                path_generation,
            };
            path.push(stamp);
            bindings.push(CandidateOpenBinding { stamp });
        }
        let terminal = *path.last().ok_or(SourceBoundLedgerError::BindingNotOpen)?;
        let line_ordinal = coordinates.current_completed_line_ordinal;
        let pending_line_ordinal = line_ordinal
            .checked_sub(1)
            .ok_or(SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?;
        let pending = PendingTerminator {
            debug_permit: None,
            atom: CertifiedAtom {
                build: epoch.build_id(),
                line_ordinal: pending_line_ordinal,
                line_start,
                absolute_start: accepted_start,
                absolute_end: physical_metric.bytes,
                metric_after: physical_metric,
                special_after: draft.special,
                kind: CandidateSourceAtomKind::LineEnding(CandidateLineEnding::Lf),
            },
            terminal,
            affinity: GreenAffinity::Downstream,
        };
        let next_path_generation = u64::try_from(path.len())
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow(
                "restored next path generation",
            ))?;
        let structural_state_generation = next_path_generation;
        let path_logical_metrics = draft.path_logical_metrics.into_vec();
        let maximum_open_path_capacity = path.capacity();
        let decoder_state = DecoderLineBoundaryState {
            cursor_metrics: CursorMetrics::default(),
            maximum_decoder_bytes: 0,
            eof_observed: false,
        };
        let authoritative_decoder = AtomDecoder::from_emitted_boundary(
            epoch.source(),
            authoritative,
            physical_metric.bytes,
            physical_metric,
            draft.special,
            decoder_state,
        );
        let recognition_decoder = AtomDecoder::from_emitted_boundary(
            epoch.source(),
            recognition,
            physical_metric.bytes,
            physical_metric,
            draft.special,
            decoder_state,
        );
        let ledger = CandidateSourceLedger {
            epoch,
            descriptor: epoch.source(),
            authoritative_root_utf16,
            authoritative: authoritative_decoder,
            recognition: RecognitionCursor {
                decoder: recognition_decoder,
                line: RecognitionLineState::new(
                    line_ordinal,
                    physical_metric.bytes,
                    physical_metric,
                ),
                expected_replay: None,
                open_range: None,
                byte_session: None,
                maximum_lead_bytes: 0,
            },
            metric: physical_metric,
            special: draft.special,
            line: LineState::new(line_ordinal, physical_metric.bytes, physical_metric),
            next_claim_offset: physical_metric.bytes,
            next_claim_metric: physical_metric,
            next_claim_special: draft.special,
            pending_terminator: Some(pending),
            pending_gap: None,
            path,
            path_logical_metrics,
            next_path_generation,
            structural_state_generation,
            maximum_open_path_len: bindings.len(),
            maximum_open_path_capacity,
            last_line_length: draft.last_line_length,
            line_count: line_ordinal,
            // Restart diagnostics are suffix-local. The cumulative physical
            // metric and logical path are retained above; old claims are not
            // replayed into the new composer receipt.
            claim_count: 0,
            debug_digest: FNV_OFFSET_BASIS,
            eof_observed: false,
            sealed: false,
        };
        Ok(RestoredSetextSourceLedger {
            ledger,
            bindings,
            #[cfg(feature = "exact-parser")]
            donor_cursor,
        })
    }

    /// Reconstructs the narrow persisted nonzero LF restart without any
    /// transient old writer/source draft.
    ///
    /// Cumulative physical coordinates come from the parent-selected cut and
    /// current SourceStore join. Every block identity, donor-facing kind, and
    /// logical fold comes from the same normalized `CurrentRestartPath`.
    /// History-only diagnostics and special-character counters deliberately
    /// restart at zero: all later delta checks are suffix-local, so rebuilding
    /// a document prefix merely to recover historical tab/NUL counts would add
    /// cost without adding semantic authority.
    #[cfg(feature = "exact-parser")]
    #[allow(clippy::too_many_lines)] // One firewall validates and remints every ledger field atomically.
    pub(crate) fn restore_parent_selected_lf(
        epoch: LiveCandidateEpoch,
        preferred: crate::retained_restart_coordinate::PreferredPersistedDeferredLfRestart,
    ) -> Result<RestoredPersistedSourceLedger, SourceBoundLedgerError> {
        let (
            _old_source,
            current_source,
            coordinates,
            previous,
            cursors,
            path,
            restart_anchor,
            parent_selection,
            source_receipt,
        ) = preferred.into_reconstruction_parts();
        Self::restore_parent_selected_persisted(
            epoch,
            PersistedSourceRestartParts {
                current_source,
                coordinates,
                previous,
                cursors,
                path,
                restart_anchor,
                parent_selection,
                source_receipt,
            },
            PersistedSourceRestartShape::DeferredLf,
        )
    }

    /// Reconstructs a persisted A=P line frontier. The selected current-green
    /// path is already finalized, the parser grammar has no deferred source,
    /// and the source ledger starts directly at the next physical line.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn restore_parent_selected_source_complete_line_boundary(
        epoch: LiveCandidateEpoch,
        preferred: crate::retained_restart_coordinate::PreferredPersistedSourceCompleteLineBoundaryRestart,
    ) -> Result<RestoredPersistedSourceLedger, SourceBoundLedgerError> {
        let (
            _old_source,
            current_source,
            coordinates,
            previous,
            cursors,
            path,
            restart_anchor,
            parent_selection,
            source_receipt,
        ) = preferred.into_reconstruction_parts();
        Self::restore_parent_selected_persisted(
            epoch,
            PersistedSourceRestartParts {
                current_source,
                coordinates,
                previous,
                cursors,
                path,
                restart_anchor,
                parent_selection,
                source_receipt,
            },
            PersistedSourceRestartShape::SourceCompleteLineBoundary,
        )
    }

    #[cfg(feature = "exact-parser")]
    #[allow(clippy::too_many_lines)] // One firewall validates and remints every ledger field atomically.
    fn restore_parent_selected_persisted(
        epoch: LiveCandidateEpoch,
        parts: PersistedSourceRestartParts,
        shape: PersistedSourceRestartShape,
    ) -> Result<RestoredPersistedSourceLedger, SourceBoundLedgerError> {
        let PersistedSourceRestartParts {
            current_source,
            coordinates,
            previous,
            cursors,
            path,
            restart_anchor,
            parent_selection,
            source_receipt,
        } = parts;
        match shape {
            PersistedSourceRestartShape::DeferredLf => {
                validate_persisted_lf_path_shape(&path, coordinates)?;
                if coordinates.current_accepted_projection_cut.checked_add(1)
                    != Some(coordinates.current_physical_restart)
                    || coordinates.current_accepted_projection_utf16.checked_add(1)
                        != Some(coordinates.current_physical_restart_utf16)
                {
                    return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
                }
            }
            PersistedSourceRestartShape::SourceCompleteLineBoundary => {
                validate_persisted_source_complete_line_boundary_path_shape(&path, coordinates)?;
                if coordinates.current_accepted_projection_cut
                    != coordinates.current_physical_restart
                    || coordinates.current_accepted_projection_utf16
                        != coordinates.current_physical_restart_utf16
                {
                    return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
                }
            }
        }
        if current_source != epoch.source()
            || coordinates.affinity != crate::BoundaryAffinity::Before
            || coordinates.parent_completed_line_ordinal
                != coordinates.current_completed_line_ordinal
            || coordinates.current_completed_line_ordinal == 0
            || coordinates.current_previous_content_bytes != previous.content_bytes()
            || coordinates.current_previous_content_utf16 != previous.content_utf16()
        {
            return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
        }

        let pair = cursors;
        if pair.descriptor() != epoch.source()
            || pair.offset() != coordinates.current_physical_restart
            || !pair.is_physical_line_start()
        {
            return Err(SourceBoundLedgerError::WrongSourceOffset);
        }
        let authoritative_root_utf16 = u64::try_from(pair.total_utf16())
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?;
        let (authoritative, recognition) = pair.into_cursors();
        let physical_metric = SourceLedgerMetric {
            bytes: u64::try_from(coordinates.current_physical_restart)
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
            utf16: u64::try_from(coordinates.current_physical_restart_utf16)
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
        };
        let accepted_metric = SourceLedgerMetric {
            bytes: u64::try_from(coordinates.current_accepted_projection_cut)
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
            utf16: u64::try_from(coordinates.current_accepted_projection_utf16)
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
        };
        let line_start = match shape {
            PersistedSourceRestartShape::DeferredLf => {
                accepted_metric
                    .utf16
                    .checked_sub(previous.content_utf16())
                    .ok_or(SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?;
                accepted_metric
                    .bytes
                    .checked_sub(previous.content_bytes())
                    .ok_or(SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?
            }
            PersistedSourceRestartShape::SourceCompleteLineBoundary => physical_metric.bytes,
        };

        let frames = path.frames();
        let mut path_stamps = Vec::new();
        path_stamps
            .try_reserve_exact(frames.len())
            .map_err(|_| SourceBoundLedgerError::Invariant("persisted path allocation"))?;
        let mut bindings = Vec::new();
        bindings
            .try_reserve_exact(frames.len())
            .map_err(|_| SourceBoundLedgerError::Invariant("persisted binding allocation"))?;
        let mut path_logical_metrics = Vec::new();
        path_logical_metrics
            .try_reserve_exact(frames.len())
            .map_err(|_| SourceBoundLedgerError::Invariant("persisted metric allocation"))?;
        for (depth, frame) in frames.iter().enumerate() {
            let kind = persisted_donor_green_kind(frame)?;
            if frame.block().0 == 0 {
                return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
            }
            let path_generation = u64::try_from(depth)
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
                .checked_add(1)
                .ok_or(SourceBoundLedgerError::Overflow(
                    "persisted path generation",
                ))?;
            let stamp = BindingStamp {
                build: epoch.build_id(),
                block: frame.block(),
                kind,
                depth: u32::try_from(depth)
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
                path_generation,
            };
            let logical = frame.logical_metric();
            path_stamps.push(stamp);
            bindings.push(CandidateOpenBinding { stamp });
            path_logical_metrics.push(SourceLedgerMetric {
                bytes: logical.bytes,
                utf16: logical.utf16,
            });
        }
        match shape {
            PersistedSourceRestartShape::DeferredLf
                if path_stamps.first().map(|stamp| stamp.kind) != Some(GreenKind::DOCUMENT)
                    || path_stamps.last().map(|stamp| stamp.kind) != Some(GreenKind::PARAGRAPH)
                    || path_logical_metrics.last().copied() != Some(accepted_metric) =>
            {
                return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
            }
            PersistedSourceRestartShape::SourceCompleteLineBoundary
                if path_stamps.first().map(|stamp| stamp.kind) != Some(GreenKind::DOCUMENT)
                    || path
                        .frames()
                        .iter()
                        .any(|frame| frame.normalization().is_some()) =>
            {
                return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
            }
            _ => {}
        }

        let line_ordinal = coordinates.current_completed_line_ordinal;
        let special = SpecialCounts::default();
        let pending = match shape {
            PersistedSourceRestartShape::DeferredLf => {
                let terminal = *path_stamps
                    .last()
                    .ok_or(SourceBoundLedgerError::BindingNotOpen)?;
                let pending_line_ordinal = line_ordinal
                    .checked_sub(1)
                    .ok_or(SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?;
                Some(PendingTerminator {
                    debug_permit: None,
                    atom: CertifiedAtom {
                        build: epoch.build_id(),
                        line_ordinal: pending_line_ordinal,
                        line_start,
                        absolute_start: accepted_metric.bytes,
                        absolute_end: physical_metric.bytes,
                        metric_after: physical_metric,
                        special_after: special,
                        kind: CandidateSourceAtomKind::LineEnding(CandidateLineEnding::Lf),
                    },
                    terminal,
                    affinity: GreenAffinity::Downstream,
                })
            }
            PersistedSourceRestartShape::SourceCompleteLineBoundary => None,
        };
        let next_path_generation = u64::try_from(path_stamps.len())
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow(
                "persisted next path generation",
            ))?;
        let maximum_open_path_capacity = path_stamps.capacity();
        let decoder_state = DecoderLineBoundaryState {
            cursor_metrics: CursorMetrics::default(),
            maximum_decoder_bytes: 0,
            eof_observed: false,
        };
        let authoritative_decoder = AtomDecoder::from_emitted_boundary(
            epoch.source(),
            authoritative,
            physical_metric.bytes,
            physical_metric,
            special,
            decoder_state,
        );
        let recognition_decoder = AtomDecoder::from_emitted_boundary(
            epoch.source(),
            recognition,
            physical_metric.bytes,
            physical_metric,
            special,
            decoder_state,
        );
        let donor_cursor =
            DirectLineBoundaryResumeCursor::new(line_ordinal, previous.content_bytes())
                .map_err(|_| SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?;
        let receipt = PersistedSourceLedgerReconstructionReceipt {
            path_frames_consumed: path_stamps.len(),
            logical_metrics_consumed: path_logical_metrics.len(),
            suffix_local_tab_baseline: special.tabs,
            suffix_local_nul_baseline: special.nuls,
            suffix_local_line_ending_baseline: special.line_endings,
            suffix_local_claim_baseline: 0,
            retained_old_source_bytes: 0,
            retained_old_writer_drafts: 0,
            parallel_restart_paths: 0,
        };
        let ledger = CandidateSourceLedger {
            epoch,
            descriptor: epoch.source(),
            authoritative_root_utf16,
            authoritative: authoritative_decoder,
            recognition: RecognitionCursor {
                decoder: recognition_decoder,
                line: RecognitionLineState::new(
                    line_ordinal,
                    physical_metric.bytes,
                    physical_metric,
                ),
                expected_replay: None,
                open_range: None,
                byte_session: None,
                maximum_lead_bytes: 0,
            },
            metric: physical_metric,
            special,
            line: LineState::new(line_ordinal, physical_metric.bytes, physical_metric),
            next_claim_offset: physical_metric.bytes,
            next_claim_metric: physical_metric,
            next_claim_special: special,
            pending_terminator: pending,
            pending_gap: None,
            path: path_stamps,
            path_logical_metrics,
            next_path_generation,
            structural_state_generation: next_path_generation,
            maximum_open_path_len: bindings.len(),
            maximum_open_path_capacity,
            last_line_length: usize::try_from(previous.content_bytes())
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?,
            line_count: line_ordinal,
            claim_count: 0,
            debug_digest: FNV_OFFSET_BASIS,
            eof_observed: false,
            sealed: false,
        };
        Ok(RestoredPersistedSourceLedger {
            parent_selection,
            ledger,
            bindings,
            donor_cursor,
            path,
            restart_anchor,
            source_receipt,
            receipt,
        })
    }

    pub(crate) fn source_identity(&self) -> crate::SourceRootId {
        self.authoritative.cursor.source_identity()
    }

    pub(crate) const fn descriptor(&self) -> SourceSnapshotDescriptor {
        self.descriptor
    }

    pub(crate) fn cursor_offset(&self) -> usize {
        self.authoritative.cursor.offset()
    }

    /// Actor-observed cumulative physical source metric. This is exposed only
    /// to the writer's retained-restart join, where it must match the opaque
    /// parent-selected checkpoint on all five axes before suffix sampling can
    /// begin.
    pub(crate) const fn physical_metric(&self) -> SourceLedgerMetric {
        self.metric
    }

    /// Observes the exact staged terminator only while it still belongs to
    /// the supplied live terminal binding. The returned value cannot resolve
    /// or publish the source atom; it exists solely to include that atom in
    /// the parser-owned reference-prefix projection before the terminal join.
    pub(crate) fn pending_terminator_for(
        &self,
        epoch: LiveCandidateEpoch,
        terminal: &CandidateOpenBinding,
    ) -> Result<Option<CandidatePendingTerminator>, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.validate_binding(terminal.stamp)?;
        let Some(pending) = self.pending_terminator.as_ref() else {
            return Ok(None);
        };
        if pending.terminal != terminal.stamp {
            return Err(SourceBoundLedgerError::Invariant(
                "pending terminator belongs to another terminal binding",
            ));
        }
        let CandidateSourceAtomKind::LineEnding(ending) = pending.atom.kind else {
            return Err(SourceBoundLedgerError::Invariant("pending terminator kind"));
        };
        let metric = SourceLedgerMetric {
            bytes: ending.bytes(),
            utf16: ending.bytes(),
        };
        Ok(Some(CandidatePendingTerminator {
            source_start: pending.atom.metric_after.checked_sub(metric)?,
            ending,
        }))
    }

    pub(crate) fn physical_metric_since(
        &self,
        earlier: SerializedMetric,
    ) -> Result<SerializedMetric, SourceBoundLedgerError> {
        let earlier = SourceLedgerMetric {
            bytes: earlier.bytes,
            utf16: earlier.utf16,
        };
        let metric = self.metric.checked_sub(earlier)?;
        Ok(SerializedMetric {
            bytes: metric.bytes,
            utf16: metric.utf16,
        })
    }

    /// Actor-owned completed physical-line count at the same authoritative
    /// cut as `physical_metric`. It is exposed only for opaque convergence
    /// target comparison inside the candidate writer.
    #[cfg(feature = "exact-parser")]
    pub(crate) const fn physical_line_ordinal(&self) -> u64 {
        self.line_count
    }

    pub(crate) const fn authoritative_root_utf16(&self) -> u64 {
        self.authoritative_root_utf16
    }

    #[cfg(test)]
    pub(crate) fn test_top_binding_state(
        &self,
    ) -> Option<(BlockId, GreenKind, u32, u64, SourceLedgerMetric, u64)> {
        let stamp = *self.path.last()?;
        let metric = *self.path_logical_metrics.last()?;
        Some((
            stamp.block,
            stamp.kind,
            stamp.depth,
            stamp.path_generation,
            metric,
            self.structural_state_generation,
        ))
    }

    pub(crate) fn has_pending_gap(&self) -> bool {
        self.pending_gap.is_some()
    }

    /// Checks the exact quiescent seam used by the same-build line-boundary
    /// continuation. This is separate from consuming the ledger so the actor
    /// can reject an unsafe pause without dropping live source authority.
    #[allow(dead_code)] // Called by the actor seam before composite-checkpoint integration.
    pub(crate) fn validate_line_boundary_continuation(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.require_no_recognition_byte_session()?;
        if self.recognition.expected_replay.is_some() {
            return Err(SourceBoundLedgerError::RecognitionReplayPending);
        }
        if self.recognition.open_range.is_some() {
            return Err(SourceBoundLedgerError::RecognitionRangeAlreadyOpen);
        }
        if self.line_count == 0
            || self.line_count != self.line.ordinal
            || self.line.has_atoms
            || self.line.last_nonblank_end != self.line.start
            || self.line.ending_atom.is_some()
            || self.line.eof
            || self.line.atom_count != 0
            || self.line.atom_debug_digest != FNV_OFFSET_BASIS
            || self.line.metric_at_start != self.metric
            || self.next_claim_offset != self.line.start
            || self.next_claim_metric != self.metric
            || self.next_claim_special != self.special
            || self.metric.bytes != self.line.start
        {
            return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
        }
        if self.recognition.line.ordinal != self.line.ordinal
            || self.recognition.line.start != self.line.start
            || self.recognition.line.metric_at_start != self.metric
            || self.recognition.line.has_atoms
            || self.recognition.line.ending.is_some()
            || self.recognition.line.eof
            || self.recognition.line.atom_count != 0
            || self.recognition.line.atom_debug_digest != FNV_OFFSET_BASIS
        {
            return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
        }
        self.authoritative.validate_emitted_line_boundary(
            self.line.start,
            self.metric,
            self.special,
            self.eof_observed,
        )?;
        self.recognition.decoder.validate_emitted_line_boundary(
            self.line.start,
            self.metric,
            self.special,
            self.eof_observed,
        )?;
        self.validate_continuation_path()?;
        self.validate_continuation_pending()?;
        Ok(())
    }

    #[allow(dead_code)] // Called by the actor seam before composite-checkpoint integration.
    pub(crate) fn line_boundary_offset_for_actor(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<u64, SourceBoundLedgerError> {
        self.validate_line_boundary_continuation(epoch)?;
        Ok(self.line.start)
    }

    #[allow(dead_code)] // Called by the actor seam before composite-checkpoint integration.
    fn validate_continuation_path(&self) -> Result<(), SourceBoundLedgerError> {
        if self.path.len() != self.path_logical_metrics.len()
            || self.next_path_generation == 0
            || self.structural_state_generation == 0
        {
            return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
        }
        for (depth, stamp) in self.path.iter().copied().enumerate() {
            if stamp.build != self.epoch.build_id()
                || usize::try_from(stamp.depth).ok() != Some(depth)
                || !is_known_kind(stamp.kind)
                || stamp.path_generation >= self.next_path_generation
                || (depth == 0 && stamp.kind != GreenKind::DOCUMENT)
                || (depth > 0 && stamp.kind == GreenKind::DOCUMENT)
            {
                return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
            }
        }
        Ok(())
    }

    #[allow(dead_code)] // Called by the actor seam before composite-checkpoint integration.
    fn validate_continuation_pending(&self) -> Result<(), SourceBoundLedgerError> {
        if self.pending_terminator.is_some() && self.pending_gap.is_some() {
            return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
        }
        if let Some(pending) = &self.pending_terminator {
            let expected_line = pending.atom.line_ordinal.checked_add(1).ok_or(
                SourceBoundLedgerError::Overflow("pending terminator line ordinal"),
            )?;
            if pending.atom.build != self.epoch.build_id()
                || expected_line != self.line.ordinal
                || pending.atom.absolute_end != self.line.start
                || pending.atom.metric_after != self.metric
                || pending.atom.special_after != self.special
                || !matches!(pending.atom.kind, CandidateSourceAtomKind::LineEnding(_))
                || self.path.get(
                    usize::try_from(pending.terminal.depth)
                        .map_err(|_| SourceBoundLedgerError::LineBoundaryContinuationUnavailable)?,
                ) != Some(&pending.terminal)
                || !is_logical_terminal(pending.terminal.kind)
                || pending
                    .debug_permit
                    .as_ref()
                    .is_some_and(|permit| permit.build_id() != self.epoch.build_id())
            {
                return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
            }
        }
        if self.pending_gap.as_ref().is_some_and(|gap| {
            gap.start >= gap.end
                || gap.end != self.line.start
                || gap.first_line >= self.line.ordinal
                || gap.binding_generation_ceiling > self.next_path_generation
                || gap.metric.bytes != gap.end.saturating_sub(gap.start)
                || gap.metric.utf16 != gap.metric.bytes
                || gap
                    .debug_permit
                    .as_ref()
                    .is_some_and(|permit| permit.build_id() != self.epoch.build_id())
        }) {
            return Err(SourceBoundLedgerError::LineBoundaryContinuationUnavailable);
        }
        Ok(())
    }

    /// Consumes the sole live ledger into a linear same-build continuation.
    /// The actor must call `validate_line_boundary_continuation` before taking
    /// the ledger so a rejected pause leaves the candidate usable.
    #[allow(dead_code)] // Called by the actor seam before composite-checkpoint integration.
    pub(crate) fn into_line_boundary_continuation(
        self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateSourceLineBoundaryContinuation, SourceBoundLedgerError> {
        self.validate_line_boundary_continuation(epoch)?;
        let authoritative_decoder = DecoderLineBoundaryState {
            cursor_metrics: self.authoritative.cumulative_cursor_metrics()?,
            maximum_decoder_bytes: self.authoritative.maximum_decoder_bytes,
            eof_observed: self.authoritative.eof_observed,
        };
        let recognition_decoder = DecoderLineBoundaryState {
            cursor_metrics: self.recognition.decoder.cumulative_cursor_metrics()?,
            maximum_decoder_bytes: self.recognition.decoder.maximum_decoder_bytes,
            eof_observed: self.recognition.decoder.eof_observed,
        };
        let Self {
            epoch,
            descriptor,
            authoritative_root_utf16,
            authoritative: _,
            recognition,
            metric,
            special,
            line,
            next_claim_offset: _,
            next_claim_metric: _,
            next_claim_special: _,
            pending_terminator,
            pending_gap,
            path,
            path_logical_metrics,
            next_path_generation,
            structural_state_generation,
            maximum_open_path_len,
            maximum_open_path_capacity,
            last_line_length,
            line_count,
            claim_count,
            debug_digest,
            eof_observed,
            sealed: _,
        } = self;
        Ok(CandidateSourceLineBoundaryContinuation {
            epoch,
            descriptor,
            authoritative_root_utf16,
            absolute_offset: line.start,
            line_ordinal: line.ordinal,
            last_line_length,
            metric,
            special,
            pending_terminator,
            pending_gap,
            path: path.into_boxed_slice(),
            path_logical_metrics: path_logical_metrics.into_boxed_slice(),
            next_path_generation,
            structural_state_generation,
            maximum_open_path_len,
            maximum_open_path_capacity,
            line_count,
            claim_count,
            debug_digest,
            authoritative_decoder,
            recognition_decoder,
            recognition_maximum_lead_bytes: recognition.maximum_lead_bytes,
            eof_observed,
        })
    }

    pub(crate) fn recognition_checkpoint(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionCheckpoint, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        Ok(self.current_recognition_checkpoint())
    }

    /// Returns the exact untouched physical-line start currently shared by
    /// speculative recognition and authoritative replay.
    ///
    /// This is deliberately narrower than `recognition_checkpoint`: a parser
    /// may use the returned opaque checkpoint to bind an actor-resolved line
    /// descriptor, but cannot ask for a new descriptor after recognition has
    /// consumed any part of the line. A pending lone-CR replay byte is allowed
    /// because it is already bound to this exact offset and immutable root.
    pub(crate) fn recognition_line_start_checkpoint(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionCheckpoint, SourceBoundLedgerError> {
        self.require_recognition_poll(epoch)?;
        let recognition = &self.recognition;
        let line = &recognition.line;
        if recognition.open_range.is_some()
            || line.has_atoms
            || line.ending.is_some()
            || line.eof
            || line.atom_count != 0
            || line.atom_debug_digest != FNV_OFFSET_BASIS
            || recognition.decoder.emitted_offset != line.start
            || recognition.decoder.metric != line.metric_at_start
            || recognition.decoder.decode.is_some()
            || recognition.decoder.pending_cr.is_some()
            || self.line.ordinal != line.ordinal
            || self.line.start != line.start
            || self.line.has_atoms
        {
            return Err(SourceBoundLedgerError::RecognitionLineNotAtStart);
        }
        if let Some(replay) = recognition.decoder.replay {
            let replay_offset = replay.offset_u64()?;
            if replay.root != self.descriptor.root
                || replay_offset != line.start
                || recognition.decoder.read_offset
                    != line
                        .start
                        .checked_add(1)
                        .ok_or(SourceBoundLedgerError::Overflow(
                            "recognition replay offset",
                        ))?
            {
                return Err(SourceBoundLedgerError::RecognitionLineNotAtStart);
            }
        } else if recognition.decoder.read_offset != line.start {
            return Err(SourceBoundLedgerError::RecognitionLineNotAtStart);
        }
        Ok(self.current_recognition_checkpoint())
    }

    fn current_recognition_checkpoint(&self) -> CandidateRecognitionCheckpoint {
        CandidateRecognitionCheckpoint {
            descriptor: self.descriptor,
            build: self.epoch.build_id(),
            line_ordinal: self.recognition.line.ordinal,
            absolute_offset: self.recognition.decoder.emitted_offset,
        }
    }

    /// Begins the sole candidate-owned byte view from an actor-joined,
    /// untouched-line binding. Every identity component is rechecked after
    /// the descriptor crosses back into the mutable actor call.
    pub(crate) fn begin_recognition_byte_session(
        &mut self,
        epoch: LiveCandidateEpoch,
        bound_epoch: LiveCandidateEpoch,
        checkpoint: CandidateRecognitionCheckpoint,
        physical: SourcePhysicalLineDescriptor,
    ) -> Result<CandidateRecognitionByteSession, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        if self.recognition.byte_session.is_some() {
            return Err(SourceBoundLedgerError::RecognitionByteSessionAlreadyOpen);
        }
        let current = self.recognition_line_start_checkpoint(epoch)?;
        let start = usize::try_from(current.absolute_offset())
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?;
        let ending_bytes = physical.ending().bytes();
        if bound_epoch != epoch
            || checkpoint != current
            || checkpoint.source() != self.descriptor
            || checkpoint.build_id() != self.epoch.build_id()
            || checkpoint.line_ordinal() != self.recognition.line.ordinal
            || physical.source() != self.descriptor
            || physical.start() != start
            || physical.start() > physical.content_end()
            || physical.content_end() > physical.end()
            || physical.end().checked_sub(physical.content_end()) != Some(ending_bytes)
            || physical.end() > self.descriptor.bytes
            || physical
                .physical_utf16()
                .checked_sub(physical.content_utf16())
                != Some(ending_bytes)
            || (physical.ending() == SourcePhysicalLineEnding::BareEof
                && physical.end() != self.descriptor.bytes)
        {
            return Err(SourceBoundLedgerError::RecognitionByteLineMismatch);
        }
        if physical.start() == physical.end()
            && physical.ending() == SourcePhysicalLineEnding::BareEof
        {
            return Err(SourceBoundLedgerError::RecognitionByteEmptyBareEof);
        }
        let identity = CandidateRecognitionByteSession {
            epoch,
            source: self.descriptor,
            line_ordinal: checkpoint.line_ordinal(),
            start: physical.start(),
            content_end: physical.content_end(),
            end: physical.end(),
            content_utf16: physical.content_utf16(),
            physical_utf16: physical.physical_utf16(),
            ending: physical.ending(),
        };
        self.recognition.byte_session = Some(RecognitionByteSessionState::new(identity));
        Ok(identity)
    }

    fn require_recognition_byte_session(
        &self,
        epoch: LiveCandidateEpoch,
        session: CandidateRecognitionByteSession,
    ) -> Result<&RecognitionByteSessionState, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        let active = self
            .recognition
            .byte_session
            .as_ref()
            .ok_or(SourceBoundLedgerError::RecognitionByteSessionNotOpen)?;
        if session != active.identity
            || session.epoch() != epoch
            || session.source() != self.descriptor
            || session.build_id() != self.epoch.build_id()
            || session.line_ordinal() != self.recognition.line.ordinal
            || u64::try_from(session.start()).ok() != Some(self.recognition.line.start)
        {
            return Err(SourceBoundLedgerError::RecognitionByteWrongSession);
        }
        if active.failed {
            return Err(SourceBoundLedgerError::RecognitionByteSessionFailed);
        }
        Ok(active)
    }

    fn fail_recognition_byte_session(&mut self) {
        if let Some(active) = &mut self.recognition.byte_session {
            active.failed = true;
        }
    }

    fn recognition_byte_access(
        &mut self,
        session: CandidateRecognitionByteSession,
        local_offset: usize,
    ) -> Result<RecognitionByteAccess, CandidateRecognitionByteAccessError> {
        let active = self.recognition.byte_session.as_ref().ok_or(
            CandidateRecognitionByteAccessError::Infrastructure(
                SourceBoundLedgerError::RecognitionByteSessionNotOpen,
            ),
        )?;
        if active.identity != session {
            self.fail_recognition_byte_session();
            return Err(CandidateRecognitionByteAccessError::Infrastructure(
                SourceBoundLedgerError::RecognitionByteWrongSession,
            ));
        }
        if active.failed {
            return Err(CandidateRecognitionByteAccessError::SessionFailed);
        }
        let exposed_high_water = active.exposed_high_water;
        let last_byte = active.last_byte;
        if local_offset >= session.len() {
            self.fail_recognition_byte_session();
            return Err(CandidateRecognitionByteAccessError::LogicalEof {
                requested: local_offset,
                len: session.len(),
            });
        }
        if local_offset.checked_add(1) == Some(exposed_high_water) {
            let byte = last_byte.ok_or_else(|| {
                self.fail_recognition_byte_session();
                CandidateRecognitionByteAccessError::SessionFailed
            })?;
            let active = self
                .recognition
                .byte_session
                .as_mut()
                .expect("the active byte session was checked");
            active.total_access_work_units = active.total_access_work_units.checked_add(1).ok_or(
                CandidateRecognitionByteAccessError::Infrastructure(
                    SourceBoundLedgerError::Overflow("recognition byte access work"),
                ),
            )?;
            active.repeated_last_byte_peeks = active
                .repeated_last_byte_peeks
                .checked_add(1)
                .ok_or(CandidateRecognitionByteAccessError::Infrastructure(
                    SourceBoundLedgerError::Overflow("recognition repeated byte peeks"),
                ))?;
            return Ok(RecognitionByteAccess {
                byte,
                new_byte: false,
                source_bytes_read: 0,
                decoded_atoms: 0,
            });
        }
        if local_offset != exposed_high_water {
            self.fail_recognition_byte_session();
            return Err(CandidateRecognitionByteAccessError::OutOfOrder {
                requested: local_offset,
                next_sequential: exposed_high_water,
            });
        }

        let absolute_start = u64::try_from(session.start()).map_err(|_| {
            CandidateRecognitionByteAccessError::Infrastructure(
                SourceBoundLedgerError::SourceLengthOverflow,
            )
        })?;
        let local_offset_u64 = u64::try_from(local_offset).map_err(|_| {
            CandidateRecognitionByteAccessError::Infrastructure(
                SourceBoundLedgerError::SourceLengthOverflow,
            )
        })?;
        let absolute_offset = absolute_start.checked_add(local_offset_u64).ok_or(
            CandidateRecognitionByteAccessError::Infrastructure(SourceBoundLedgerError::Overflow(
                "recognition byte absolute offset",
            )),
        )?;
        let (byte, atom, receipt) = match self
            .recognition
            .decoder
            .consume_exposed_byte(absolute_offset)
        {
            Ok(step) => step,
            Err(error) => {
                self.fail_recognition_byte_session();
                return Err(CandidateRecognitionByteAccessError::Infrastructure(error));
            }
        };
        let decoded_atoms = usize::from(atom.is_some());
        if let Some(atom) = atom
            && let Err(error) = self.record_recognition_atom(atom)
        {
            self.fail_recognition_byte_session();
            return Err(CandidateRecognitionByteAccessError::Infrastructure(error));
        }
        let active = self
            .recognition
            .byte_session
            .as_mut()
            .expect("the active byte session was checked");
        if active.identity != session || active.exposed_high_water != local_offset {
            active.failed = true;
            return Err(CandidateRecognitionByteAccessError::Infrastructure(
                SourceBoundLedgerError::RecognitionByteWrongSession,
            ));
        }
        active.exposed_high_water = local_offset.checked_add(1).ok_or(
            CandidateRecognitionByteAccessError::Infrastructure(SourceBoundLedgerError::Overflow(
                "recognition exposed high-water",
            )),
        )?;
        active.last_byte = Some(byte);
        active.total_access_work_units = active.total_access_work_units.checked_add(1).ok_or(
            CandidateRecognitionByteAccessError::Infrastructure(SourceBoundLedgerError::Overflow(
                "recognition byte access work",
            )),
        )?;
        active.new_bytes = active.new_bytes.checked_add(1).ok_or(
            CandidateRecognitionByteAccessError::Infrastructure(SourceBoundLedgerError::Overflow(
                "recognition new bytes",
            )),
        )?;
        active.source_bytes_read = active
            .source_bytes_read
            .checked_add(u64::try_from(receipt.source_bytes_read).map_err(|_| {
                CandidateRecognitionByteAccessError::Infrastructure(
                    SourceBoundLedgerError::SourceLengthOverflow,
                )
            })?)
            .ok_or(CandidateRecognitionByteAccessError::Infrastructure(
                SourceBoundLedgerError::Overflow("recognition source bytes read"),
            ))?;
        active.decoded_atoms = active
            .decoded_atoms
            .checked_add(u64::try_from(decoded_atoms).map_err(|_| {
                CandidateRecognitionByteAccessError::Infrastructure(
                    SourceBoundLedgerError::SourceLengthOverflow,
                )
            })?)
            .ok_or(CandidateRecognitionByteAccessError::Infrastructure(
                SourceBoundLedgerError::Overflow("recognition decoded atoms"),
            ))?;
        Ok(RecognitionByteAccess {
            byte,
            new_byte: true,
            source_bytes_read: receipt.source_bytes_read,
            decoded_atoms,
        })
    }

    pub(crate) fn poll_recognition_byte_session<S: CandidateRecognitionByteScanner>(
        &mut self,
        epoch: LiveCandidateEpoch,
        session: CandidateRecognitionByteSession,
        fuel: usize,
        scanner: &mut S,
    ) -> Result<
        CandidateRecognitionBytePollReceipt,
        CandidateRecognitionBytePollError<SourceBoundLedgerError, S::Error>,
    > {
        let start_exposed_high_water = self
            .require_recognition_byte_session(epoch, session)
            .map_err(CandidateRecognitionBytePollError::Infrastructure)?
            .exposed_high_water;
        let mut source = CandidateRecognitionByteSource {
            ledger: self,
            session,
            budget: fuel.min(CANDIDATE_RECOGNITION_BYTE_POLL_MAX_ACCESSES),
            start_exposed_high_water,
            access_work_units: 0,
            new_bytes: 0,
            source_bytes_read: 0,
            repeated_last_byte_peeks: 0,
            decoded_atoms: 0,
            budget_exhausted: false,
            fatal: None,
        };
        let scanner_result = scanner.poll(&mut source);
        let fatal = source.fatal;
        let receipt = source
            .receipt()
            .map_err(CandidateRecognitionBytePollError::Infrastructure)?;
        if let Err(error) = scanner_result {
            return Err(CandidateRecognitionBytePollError::Scanner(error));
        }
        if let Some(error) = fatal {
            return Err(CandidateRecognitionBytePollError::Infrastructure(error));
        }
        Ok(receipt)
    }

    pub(crate) fn finish_recognition_byte_session(
        &mut self,
        epoch: LiveCandidateEpoch,
        session: CandidateRecognitionByteSession,
    ) -> Result<CandidateRecognitionByteSessionFinishReceipt, SourceBoundLedgerError> {
        let active = *self.require_recognition_byte_session(epoch, session)?;
        if active.exposed_high_water != session.len() {
            return Err(SourceBoundLedgerError::RecognitionByteSessionIncomplete);
        }
        let absolute_end = u64::try_from(session.end())
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?;
        if let Some(atom) = self
            .recognition
            .decoder
            .finish_bounded_line(absolute_end, session.ending())?
        {
            self.record_recognition_atom(atom)?;
        }
        if session.ending() == SourcePhysicalLineEnding::BareEof {
            self.recognition.line.eof = true;
        }
        let expected_ending = match session.ending() {
            SourcePhysicalLineEnding::Lf => Some(CandidateLineEnding::Lf),
            SourcePhysicalLineEnding::LoneCr => Some(CandidateLineEnding::LoneCr),
            SourcePhysicalLineEnding::CrLf => Some(CandidateLineEnding::CrLf),
            SourcePhysicalLineEnding::BareEof => None,
        };
        let line_metric = self
            .recognition
            .decoder
            .metric
            .checked_sub(self.recognition.line.metric_at_start)?;
        if self.recognition.line.ending != expected_ending
            || self.recognition.decoder.emitted_offset != absolute_end
            || line_metric.bytes
                != u64::try_from(session.len())
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            || line_metric.utf16
                != u64::try_from(session.physical_utf16())
                    .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
        {
            self.fail_recognition_byte_session();
            return Err(SourceBoundLedgerError::RecognitionByteLineMismatch);
        }
        let line = self.advance_recognition_line()?;
        self.recognition.expected_replay = Some(ExpectedReplay::Line(line));
        let completed = self
            .recognition
            .byte_session
            .take()
            .ok_or(SourceBoundLedgerError::RecognitionByteSessionNotOpen)?;
        Ok(CandidateRecognitionByteSessionFinishReceipt {
            line,
            session,
            total_access_work_units: completed.total_access_work_units,
            new_bytes: completed.new_bytes,
            source_bytes_read: completed.source_bytes_read,
            repeated_last_byte_peeks: completed.repeated_last_byte_peeks,
            decoded_atoms: completed.decoded_atoms,
            physical_high_water: self.recognition.decoder.read_offset,
            maximum_retained_byte_scratch: usize::from(completed.last_byte.is_some()),
        })
    }

    fn require_recognition_poll(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        if self.recognition.expected_replay.is_some() {
            return Err(SourceBoundLedgerError::RecognitionReplayPending);
        }
        if self.recognition.byte_session.is_some() {
            return Err(SourceBoundLedgerError::RecognitionByteSessionAlreadyOpen);
        }
        if self.recognition.line.ending.is_some() {
            return Err(SourceBoundLedgerError::LineAlreadyEnded);
        }
        Ok(())
    }

    fn require_no_recognition_byte_session(&self) -> Result<(), SourceBoundLedgerError> {
        if self.recognition.byte_session.is_some() {
            Err(SourceBoundLedgerError::RecognitionByteSessionAlreadyOpen)
        } else {
            Ok(())
        }
    }

    fn record_recognition_atom(
        &mut self,
        atom: DecodedAtom,
    ) -> Result<(CandidateRecognitionAtom, CandidateRecognitionCheckpoint), SourceBoundLedgerError>
    {
        self.recognition.line.has_atoms = true;
        self.recognition.line.atom_count = self
            .recognition
            .line
            .atom_count
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow("recognition atom count"))?;
        self.recognition.line.atom_debug_digest =
            fold_decoded_atom(self.recognition.line.atom_debug_digest, atom);
        if let Some(range) = &mut self.recognition.open_range {
            range.atom_count =
                range
                    .atom_count
                    .checked_add(1)
                    .ok_or(SourceBoundLedgerError::Overflow(
                        "recognition range atom count",
                    ))?;
            range.atom_debug_digest = fold_decoded_atom(range.atom_debug_digest, atom);
        }
        if let CandidateSourceAtomKind::LineEnding(ending) = atom.kind {
            self.recognition.line.ending = Some(ending);
        }
        self.recognition.maximum_lead_bytes = self.recognition.maximum_lead_bytes.max(
            atom.absolute_end
                .saturating_sub(self.authoritative.emitted_offset),
        );
        Ok((
            CandidateRecognitionAtom {
                kind: atom.kind,
                absolute_start: atom.absolute_start,
                absolute_end: atom.absolute_end,
            },
            self.current_recognition_checkpoint(),
        ))
    }

    pub(crate) fn poll_recognition(
        &mut self,
        epoch: LiveCandidateEpoch,
        fuel: usize,
    ) -> Result<CandidateRecognitionPoll, SourceBoundLedgerError> {
        self.require_recognition_poll(epoch)?;
        match self.recognition.decoder.poll(fuel)? {
            AtomDecoderPoll::NeedFuel(receipt) => Ok(CandidateRecognitionPoll::NeedFuel(receipt)),
            AtomDecoderPoll::Atom { atom, receipt } => {
                let (atom, checkpoint) = self.record_recognition_atom(atom)?;
                Ok(CandidateRecognitionPoll::Atom {
                    atom,
                    checkpoint,
                    receipt,
                })
            }
            AtomDecoderPoll::Eof(receipt) => {
                self.recognition.line.eof = true;
                Ok(CandidateRecognitionPoll::Eof(receipt))
            }
        }
    }

    /// Advances speculative recognition in one bounded actor-owned window.
    /// The decoder, digest, and line-boundary stop remain ledger-owned; the
    /// sink sees only read-only atoms and cannot mint source authority.
    pub(crate) fn poll_recognition_window<S: CandidateRecognitionSink>(
        &mut self,
        epoch: LiveCandidateEpoch,
        fuel: usize,
        sink: &mut S,
    ) -> Result<
        CandidateRecognitionWindowReceipt,
        CandidateRecognitionWindowError<SourceBoundLedgerError, S::Error>,
    > {
        self.require_recognition_poll(epoch)
            .map_err(CandidateRecognitionWindowError::Infrastructure)?;
        let start = self.current_recognition_checkpoint();
        let budget = fuel.min(CANDIDATE_RECOGNITION_WINDOW_MAX_WORK);
        let mut work = CandidateSourcePollReceipt::default();
        let mut atoms_emitted = 0_usize;

        if budget == 0 {
            return Ok(CandidateRecognitionWindowReceipt {
                start,
                end: start,
                work,
                atoms_emitted,
                status: CandidateRecognitionWindowStatus::BudgetExhausted,
            });
        }

        loop {
            let remaining = budget.checked_sub(work.work_units).ok_or(
                CandidateRecognitionWindowError::Infrastructure(SourceBoundLedgerError::Invariant(
                    "recognition window exceeded its budget",
                )),
            )?;
            if remaining == 0 {
                return Ok(CandidateRecognitionWindowReceipt {
                    start,
                    end: self.current_recognition_checkpoint(),
                    work,
                    atoms_emitted,
                    status: CandidateRecognitionWindowStatus::BudgetExhausted,
                });
            }

            let poll = self
                .recognition
                .decoder
                .poll(remaining)
                .map_err(CandidateRecognitionWindowError::Infrastructure)?;
            let receipt = match &poll {
                AtomDecoderPoll::NeedFuel(receipt)
                | AtomDecoderPoll::Atom { receipt, .. }
                | AtomDecoderPoll::Eof(receipt) => *receipt,
            };
            work.work_units = work.work_units.checked_add(receipt.work_units).ok_or(
                CandidateRecognitionWindowError::Infrastructure(SourceBoundLedgerError::Overflow(
                    "recognition window work",
                )),
            )?;
            work.source_bytes_read = work
                .source_bytes_read
                .checked_add(receipt.source_bytes_read)
                .ok_or(CandidateRecognitionWindowError::Infrastructure(
                    SourceBoundLedgerError::Overflow("recognition window source bytes"),
                ))?;

            match poll {
                AtomDecoderPoll::NeedFuel(_) => {
                    return Ok(CandidateRecognitionWindowReceipt {
                        start,
                        end: self.current_recognition_checkpoint(),
                        work,
                        atoms_emitted,
                        status: CandidateRecognitionWindowStatus::BudgetExhausted,
                    });
                }
                AtomDecoderPoll::Atom { atom, .. } => {
                    let ending = match atom.kind {
                        CandidateSourceAtomKind::LineEnding(ending) => Some(ending),
                        CandidateSourceAtomKind::Scalar(_)
                        | CandidateSourceAtomKind::Tab
                        | CandidateSourceAtomKind::Nul => None,
                    };
                    let (atom, _) = self
                        .record_recognition_atom(atom)
                        .map_err(CandidateRecognitionWindowError::Infrastructure)?;
                    atoms_emitted = atoms_emitted.checked_add(1).ok_or(
                        CandidateRecognitionWindowError::Infrastructure(
                            SourceBoundLedgerError::Overflow("recognition window atom count"),
                        ),
                    )?;
                    sink.push_recognition_atom(atom)
                        .map_err(CandidateRecognitionWindowError::Sink)?;
                    if let Some(ending) = ending {
                        return Ok(CandidateRecognitionWindowReceipt {
                            start,
                            end: self.current_recognition_checkpoint(),
                            work,
                            atoms_emitted,
                            status: CandidateRecognitionWindowStatus::LineEnded(ending),
                        });
                    }
                }
                AtomDecoderPoll::Eof(_) => {
                    self.recognition.line.eof = true;
                    return Ok(CandidateRecognitionWindowReceipt {
                        start,
                        end: self.current_recognition_checkpoint(),
                        work,
                        atoms_emitted,
                        status: CandidateRecognitionWindowStatus::Eof,
                    });
                }
            }
        }
    }

    pub(crate) fn finish_recognition_line(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionLineReceipt, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_no_recognition_byte_session()?;
        if self.recognition.expected_replay.is_some() {
            return Err(SourceBoundLedgerError::RecognitionReplayPending);
        }
        if self.recognition.open_range.is_some() {
            return Err(SourceBoundLedgerError::RecognitionRangeAlreadyOpen);
        }
        if self.recognition.line.start != self.line.start
            || self.recognition.line.ordinal != self.line.ordinal
            || self.line.has_atoms
        {
            return Err(SourceBoundLedgerError::RecognitionReplayMismatch);
        }
        let receipt = self.advance_recognition_line()?;
        self.recognition.expected_replay = Some(ExpectedReplay::Line(receipt));
        Ok(receipt)
    }

    pub(crate) fn begin_recognition_range(
        &mut self,
        epoch: LiveCandidateEpoch,
        kind: CandidateRecognitionRangeKind,
    ) -> Result<(), SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.require_no_recognition_byte_session()?;
        if self.recognition.expected_replay.is_some() {
            return Err(SourceBoundLedgerError::RecognitionReplayPending);
        }
        if self.recognition.open_range.is_some() {
            return Err(SourceBoundLedgerError::RecognitionRangeAlreadyOpen);
        }
        if self.recognition.line.has_atoms
            || self.recognition.line.start != self.line.start
            || self.recognition.line.ordinal != self.line.ordinal
            || self.line.has_atoms
        {
            return Err(SourceBoundLedgerError::RecognitionReplayMismatch);
        }
        self.recognition.open_range = Some(OpenRecognitionRange {
            kind,
            first_line: self.recognition.line.ordinal,
            absolute_start: self.recognition.line.start,
            metric_at_start: self.recognition.decoder.metric,
            line_count: 0,
            atom_count: 0,
            atom_debug_digest: FNV_OFFSET_BASIS,
        });
        Ok(())
    }

    pub(crate) fn continue_recognition_range_line(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionLineReceipt, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_no_recognition_byte_session()?;
        if self.recognition.open_range.is_none() {
            return Err(SourceBoundLedgerError::RecognitionRangeNotOpen);
        }
        let receipt = self.advance_recognition_line()?;
        let range = self
            .recognition
            .open_range
            .as_mut()
            .expect("range was checked");
        range.line_count =
            range
                .line_count
                .checked_add(1)
                .ok_or(SourceBoundLedgerError::Overflow(
                    "recognition range line count",
                ))?;
        Ok(receipt)
    }

    pub(crate) fn finish_recognition_range(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionRangeReceipt, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_no_recognition_byte_session()?;
        if self.recognition.open_range.is_none() {
            return Err(SourceBoundLedgerError::RecognitionRangeNotOpen);
        }
        let final_line = self.advance_recognition_line()?;
        let mut range = self
            .recognition
            .open_range
            .take()
            .expect("range was checked");
        range.line_count =
            range
                .line_count
                .checked_add(1)
                .ok_or(SourceBoundLedgerError::Overflow(
                    "recognition range line count",
                ))?;
        let receipt = CandidateRecognitionRangeReceipt {
            descriptor: self.descriptor,
            build: self.epoch.build_id(),
            kind: range.kind,
            first_line: range.first_line,
            line_count: range.line_count,
            absolute_start: range.absolute_start,
            absolute_end: final_line.absolute_end,
            metric: self
                .recognition
                .decoder
                .metric
                .checked_sub(range.metric_at_start)?,
            atom_count: range.atom_count,
            atom_debug_digest: range.atom_debug_digest,
        };
        self.recognition.expected_replay = Some(ExpectedReplay::Range {
            receipt,
            progress: ReplayRangeProgress {
                metric_at_start: self.metric,
                line_count: 0,
                atom_count: 0,
                atom_debug_digest: FNV_OFFSET_BASIS,
            },
        });
        Ok(receipt)
    }

    fn advance_recognition_line(
        &mut self,
    ) -> Result<CandidateRecognitionLineReceipt, SourceBoundLedgerError> {
        let end = if self.recognition.line.ending.is_some()
            || (self.recognition.line.eof && self.recognition.line.has_atoms)
        {
            self.recognition.decoder.emitted_offset
        } else {
            return Err(SourceBoundLedgerError::RecognitionLineNotEnded);
        };
        let receipt = CandidateRecognitionLineReceipt {
            descriptor: self.descriptor,
            build: self.epoch.build_id(),
            line_ordinal: self.recognition.line.ordinal,
            absolute_start: self.recognition.line.start,
            absolute_end: end,
            metric: self
                .recognition
                .decoder
                .metric
                .checked_sub(self.recognition.line.metric_at_start)?,
            ending: self.recognition.line.ending,
            atom_count: self.recognition.line.atom_count,
            atom_debug_digest: self.recognition.line.atom_debug_digest,
        };
        let next_ordinal = self
            .recognition
            .line
            .ordinal
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow("recognition line ordinal"))?;
        self.recognition.line =
            RecognitionLineState::new(next_ordinal, end, self.recognition.decoder.metric);
        Ok(receipt)
    }

    pub(crate) fn poll(
        &mut self,
        epoch: LiveCandidateEpoch,
        fuel: usize,
    ) -> Result<CandidateSourcePoll, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.require_no_recognition_byte_session()?;
        if self.line.ending_atom.is_some() {
            return Err(SourceBoundLedgerError::LineAlreadyEnded);
        }
        if self.eof_observed {
            return Ok(CandidateSourcePoll::Eof(
                CandidateSourcePollReceipt::default(),
            ));
        }
        if let Some(expected) = self.recognition.expected_replay {
            match expected {
                ExpectedReplay::Line(receipt) => {
                    if receipt.line_ordinal != self.line.ordinal
                        || receipt.absolute_start != self.line.start
                    {
                        return Err(SourceBoundLedgerError::RecognitionReplayMismatch);
                    }
                }
                ExpectedReplay::Range { receipt, .. } => {
                    if self.line.ordinal < receipt.first_line
                        || self.line.start < receipt.absolute_start
                        || self.line.start >= receipt.absolute_end
                    {
                        return Err(SourceBoundLedgerError::RecognitionReplayMismatch);
                    }
                }
            }
        }
        match self.authoritative.poll(fuel)? {
            AtomDecoderPoll::NeedFuel(receipt) => Ok(CandidateSourcePoll::NeedFuel(receipt)),
            AtomDecoderPoll::Atom { atom, receipt } => {
                let certified = self.accept_decoded_atom(atom)?;
                Ok(CandidateSourcePoll::Atom {
                    atom: CandidateSourceAtom { certified },
                    receipt,
                })
            }
            AtomDecoderPoll::Eof(receipt) => {
                self.eof_observed = true;
                self.line.eof = true;
                Ok(CandidateSourcePoll::Eof(receipt))
            }
        }
    }

    fn accept_decoded_atom(
        &mut self,
        atom: DecodedAtom,
    ) -> Result<CertifiedAtom, SourceBoundLedgerError> {
        if atom.absolute_start != self.metric.bytes {
            return Err(SourceBoundLedgerError::WrongSourceOffset);
        }
        let certified = CertifiedAtom {
            build: self.epoch.build_id(),
            line_ordinal: self.line.ordinal,
            line_start: self.line.start,
            absolute_start: atom.absolute_start,
            absolute_end: atom.absolute_end,
            metric_after: atom.metric_after,
            special_after: atom.special_after,
            kind: atom.kind,
        };
        self.metric = atom.metric_after;
        self.special = atom.special_after;
        self.line.has_atoms = true;
        self.line.atom_count = self
            .line
            .atom_count
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow("line atom count"))?;
        self.line.atom_debug_digest = fold_decoded_atom(self.line.atom_debug_digest, atom);
        if let Some(ExpectedReplay::Range { progress, .. }) = &mut self.recognition.expected_replay
        {
            progress.atom_count = progress
                .atom_count
                .checked_add(1)
                .ok_or(SourceBoundLedgerError::Overflow("replay range atom count"))?;
            progress.atom_debug_digest = fold_decoded_atom(progress.atom_debug_digest, atom);
        }
        if !matches!(
            atom.kind,
            CandidateSourceAtomKind::Scalar(' ')
                | CandidateSourceAtomKind::Tab
                | CandidateSourceAtomKind::LineEnding(_)
        ) {
            self.line.last_nonblank_end = atom.absolute_end;
        }
        if matches!(atom.kind, CandidateSourceAtomKind::LineEnding(_)) {
            self.line.ending_atom = Some(certified);
        }
        Ok(certified)
    }

    // The permit is intentionally consumed even on failure; fresh IDs burn.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn open_binding(
        &mut self,
        epoch: LiveCandidateEpoch,
        permit: FreshBlockPermit,
        kind: GreenKind,
    ) -> Result<CandidateOpenBinding, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        if permit.build_id() != self.epoch.build_id() {
            return Err(SourceBoundLedgerError::WrongBindingBuild);
        }
        if !is_known_kind(kind) {
            return Err(SourceBoundLedgerError::InvalidKind(kind));
        }
        if self.path.is_empty() && kind != GreenKind::DOCUMENT {
            return Err(SourceBoundLedgerError::BindingNotOpen);
        }
        if !self.path.is_empty() && kind == GreenKind::DOCUMENT {
            return Err(SourceBoundLedgerError::BindingNotOpen);
        }
        let generation = self.next_path_generation;
        let next_path_generation = self
            .next_path_generation
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow("open-path generation"))?;
        let structural_state_generation = self.structural_state_generation.checked_add(1).ok_or(
            SourceBoundLedgerError::Overflow("structural-state generation"),
        )?;
        let depth = u32::try_from(self.path.len())
            .map_err(|_| SourceBoundLedgerError::Overflow("open-path depth"))?;
        let stamp = BindingStamp {
            build: permit.build_id(),
            block: permit.id(),
            kind,
            depth,
            path_generation: generation,
        };
        self.next_path_generation = next_path_generation;
        self.structural_state_generation = structural_state_generation;
        self.path.push(stamp);
        self.path_logical_metrics
            .push(SourceLedgerMetric::default());
        self.maximum_open_path_len = self.maximum_open_path_len.max(self.path.len());
        self.maximum_open_path_capacity = self.maximum_open_path_capacity.max(self.path.capacity());
        Ok(CandidateOpenBinding { stamp })
    }

    pub(crate) fn close_binding(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: &CandidateOpenBinding,
    ) -> Result<(), SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.validate_binding(binding.stamp)?;
        if self.path.last() != Some(&binding.stamp) {
            return Err(SourceBoundLedgerError::CloseIsNotTop);
        }
        let structural_state_generation = self.structural_state_generation.checked_add(1).ok_or(
            SourceBoundLedgerError::Overflow("structural-state generation"),
        )?;
        self.path.pop();
        self.path_logical_metrics.pop();
        self.structural_state_generation = structural_state_generation;
        Ok(())
    }

    pub(crate) fn retire_reference_only_paragraph(
        &mut self,
        epoch: LiveCandidateEpoch,
        paragraph: CandidateOpenBinding,
    ) -> Result<CandidateReferenceOnlyRetirement, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.validate_binding(paragraph.stamp)?;
        if paragraph.stamp.kind != GreenKind::PARAGRAPH
            || self.path.last() != Some(&paragraph.stamp)
            || self.path.len() < 2
        {
            return Err(SourceBoundLedgerError::CloseIsNotTop);
        }
        let parent_stamp = self.path[self.path.len() - 2];
        let pending = match self.pending_terminator.as_ref() {
            Some(pending) => {
                if pending.terminal != paragraph.stamp || pending.debug_permit.is_some() {
                    return Err(SourceBoundLedgerError::Invariant(
                        "reference-only terminator belongs to another binding",
                    ));
                }
                Some((
                    pending.atom.line_ordinal,
                    pending.atom.absolute_start,
                    pending.atom.absolute_end,
                    pending.atom.kind,
                ))
            }
            None => None,
        };
        let structural_state_generation = self.structural_state_generation.checked_add(1).ok_or(
            SourceBoundLedgerError::Overflow("structural-state generation"),
        )?;
        self.pending_terminator = None;
        self.path.pop();
        self.path_logical_metrics.pop();
        self.structural_state_generation = structural_state_generation;
        let terminator_gap = pending
            .map(|(line_ordinal, absolute_start, absolute_end, kind)| {
                let CandidateSourceAtomKind::LineEnding(ending) = kind else {
                    return Err(SourceBoundLedgerError::Invariant(
                        "reference-only pending atom is not a terminator",
                    ));
                };
                let metric = SourceLedgerMetric {
                    bytes: ending.bytes(),
                    utf16: ending.bytes(),
                };
                let piece = self.make_piece(
                    line_ordinal,
                    absolute_start,
                    absolute_end,
                    metric,
                    parent_stamp,
                    CoveragePart::GAP,
                    CandidateLogicalAction::none().retained(),
                )?;
                self.accept_piece(piece)
            })
            .transpose()?;
        Ok(CandidateReferenceOnlyRetirement {
            parent: CandidateOpenBinding {
                stamp: parent_stamp,
            },
            terminator_gap,
        })
    }

    /// Consumes the active provisional Paragraph binding and returns the same
    /// semantic owner retyped as a Setext Heading.
    ///
    /// This is deliberately narrower than a generic kind mutation. Setext is
    /// the one direct-parser transition in the selected profile that changes
    /// the canonical kind of an already-open terminal while preserving its
    /// identity and accumulated logical metric. The previous binding becomes
    /// invalid because its complete stamp (including kind) no longer matches
    /// the live path.
    #[allow(dead_code, clippy::needless_pass_by_value)] // Awaiting the Setext writer group.
    pub(crate) fn promote_top_paragraph_to_setext_heading(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateOpenBinding,
    ) -> Result<CandidateOpenBinding, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.validate_binding(binding.stamp)?;
        if binding.stamp.kind != GreenKind::PARAGRAPH {
            return Err(SourceBoundLedgerError::InvalidKind(binding.stamp.kind));
        }
        if self.path.last() != Some(&binding.stamp) {
            return Err(SourceBoundLedgerError::CloseIsNotTop);
        }
        self.require_predecessor_resolved()?;

        let structural_state_generation = self.structural_state_generation.checked_add(1).ok_or(
            SourceBoundLedgerError::Overflow("structural-state generation"),
        )?;
        let promoted = BindingStamp {
            kind: GreenKind::HEADING,
            ..binding.stamp
        };
        *self
            .path
            .last_mut()
            .ok_or(SourceBoundLedgerError::BindingNotOpen)? = promoted;
        self.structural_state_generation = structural_state_generation;
        Ok(CandidateOpenBinding { stamp: promoted })
    }

    /// Replaces the deepest open semantic binding after storage has atomically
    /// replaced that same active canonical fragment.  The new block identity
    /// comes from the existing candidate allocator; no parser-authored raw ID
    /// or path vector can cross this seam.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn replace_top_binding(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateOpenBinding,
        replacement: FreshBlockPermit,
        kind: GreenKind,
    ) -> Result<CandidateOpenBinding, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.validate_binding(binding.stamp)?;
        if binding.stamp.kind != GreenKind::PARAGRAPH
            || replacement.build_id() != self.epoch.build_id()
            || kind == GreenKind::DOCUMENT
            || !is_known_kind(kind)
        {
            return Err(SourceBoundLedgerError::InvalidKind(kind));
        }
        if self.path.last() != Some(&binding.stamp) {
            return Err(SourceBoundLedgerError::CloseIsNotTop);
        }
        self.require_predecessor_resolved()?;

        let path_generation = self.next_path_generation;
        self.next_path_generation = self
            .next_path_generation
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow("open-path generation"))?;
        self.structural_state_generation = self.structural_state_generation.checked_add(1).ok_or(
            SourceBoundLedgerError::Overflow("structural-state generation"),
        )?;
        let rebound = BindingStamp {
            build: replacement.build_id(),
            block: replacement.id(),
            kind,
            depth: binding.stamp.depth,
            path_generation,
        };
        *self
            .path
            .last_mut()
            .ok_or(SourceBoundLedgerError::BindingNotOpen)? = rebound;
        // The retired Paragraph's aggregate inline metric is represented by
        // the replacement fragment's closed descendants.  The open container
        // root itself begins with no direct logical contribution.
        let depth =
            usize::try_from(rebound.depth).map_err(|_| SourceBoundLedgerError::BindingNotOpen)?;
        *self
            .path_logical_metrics
            .get_mut(depth)
            .ok_or(SourceBoundLedgerError::BindingNotOpen)? = SourceLedgerMetric::default();
        Ok(CandidateOpenBinding { stamp: rebound })
    }

    /// Replaces one open logical terminal with a writer-minted fresh terminal
    /// while returning the retired identity as an inseparable residual. The
    /// accumulated logical metric remains on the replacement; a later reopen
    /// starts the residual with an empty metric at its new source cut.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn replace_top_binding_with_deferred_identity(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateOpenBinding,
        replacement: FreshBlockPermit,
        kind: GreenKind,
    ) -> Result<(CandidateOpenBinding, DeferredNormalizationIdentity), SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.validate_binding(binding.stamp)?;
        if !binding.stamp.kind.is_logical_terminal()
            || !kind.shares_logical_channel(binding.stamp.kind)
            || replacement.build_id() != self.epoch.build_id()
            || replacement.id() == binding.stamp.block
            || kind == GreenKind::DOCUMENT
            || !is_known_kind(kind)
        {
            return Err(SourceBoundLedgerError::InvalidKind(kind));
        }
        if self.path.last() != Some(&binding.stamp) {
            return Err(SourceBoundLedgerError::CloseIsNotTop);
        }
        self.require_predecessor_resolved()?;
        let depth = usize::try_from(binding.stamp.depth)
            .map_err(|_| SourceBoundLedgerError::BindingNotOpen)?;
        let parent = depth
            .checked_sub(1)
            .and_then(|index| self.path.get(index).copied())
            .ok_or(SourceBoundLedgerError::BindingNotOpen)?;

        let path_generation = self.next_path_generation;
        self.next_path_generation = self
            .next_path_generation
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow("open-path generation"))?;
        self.structural_state_generation = self.structural_state_generation.checked_add(1).ok_or(
            SourceBoundLedgerError::Overflow("structural-state generation"),
        )?;
        let rebound = BindingStamp {
            build: replacement.build_id(),
            block: replacement.id(),
            kind,
            depth: binding.stamp.depth,
            path_generation,
        };
        *self
            .path
            .last_mut()
            .ok_or(SourceBoundLedgerError::BindingNotOpen)? = rebound;
        Ok((
            CandidateOpenBinding { stamp: rebound },
            DeferredNormalizationIdentity {
                build: self.epoch.build_id(),
                retired: binding.stamp,
                replacement: rebound,
                parent,
            },
        ))
    }

    /// Consumes the residual identity only after its replacement has closed
    /// and reopens it under the same authenticated parent. There is no raw-ID
    /// or caller-kind argument.
    pub(crate) fn reopen_deferred_normalization_identity(
        &mut self,
        epoch: LiveCandidateEpoch,
        identity: DeferredNormalizationIdentity,
    ) -> Result<CandidateOpenBinding, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        let expected_depth = usize::try_from(identity.retired.depth)
            .map_err(|_| SourceBoundLedgerError::BindingNotOpen)?;
        if identity.build != self.epoch.build_id()
            || !identity.retired.kind.is_logical_terminal()
            || self.path.len() != expected_depth
            || self.path.last() != Some(&identity.parent)
            || self.path.iter().any(|stamp| stamp == &identity.replacement)
        {
            return Err(SourceBoundLedgerError::BindingNotOpen);
        }
        let path_generation = self.next_path_generation;
        self.next_path_generation = self
            .next_path_generation
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow("open-path generation"))?;
        self.structural_state_generation = self.structural_state_generation.checked_add(1).ok_or(
            SourceBoundLedgerError::Overflow("structural-state generation"),
        )?;
        let reopened = BindingStamp {
            build: identity.build,
            block: identity.retired.block,
            kind: identity.retired.kind,
            depth: identity.retired.depth,
            path_generation,
        };
        self.path.push(reopened);
        self.path_logical_metrics
            .push(SourceLedgerMetric::default());
        self.maximum_open_path_len = self.maximum_open_path_len.max(self.path.len());
        self.maximum_open_path_capacity = self.maximum_open_path_capacity.max(self.path.capacity());
        Ok(CandidateOpenBinding { stamp: reopened })
    }

    /// Chooses the whole-fragment identity after lookahead proved that no
    /// residual terminal reopens. The returned token authorizes the matching
    /// hidden packed-green Enter reidentity before publication.
    pub(crate) fn resolve_deferred_normalization_whole(
        &mut self,
        epoch: LiveCandidateEpoch,
        identity: DeferredNormalizationIdentity,
    ) -> Result<ResolvedWholeNormalizationIdentity, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        let expected_depth = usize::try_from(identity.retired.depth)
            .map_err(|_| SourceBoundLedgerError::BindingNotOpen)?;
        if identity.build != self.epoch.build_id()
            || self.path.len() != expected_depth
            || self.path.last() != Some(&identity.parent)
            || self.path.iter().any(|stamp| stamp == &identity.replacement)
        {
            return Err(SourceBoundLedgerError::BindingNotOpen);
        }
        Ok(ResolvedWholeNormalizationIdentity {
            build: identity.build,
            retired_block: identity.retired.block,
            replacement_block: identity.replacement.block,
            kind: identity.replacement.kind,
        })
    }

    pub(crate) fn fenced_code_logical_metric(
        &self,
        epoch: LiveCandidateEpoch,
        binding: &CandidateOpenBinding,
    ) -> Result<SourceLedgerMetric, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.validate_binding(binding.stamp)?;
        if binding.kind() != GreenKind::FENCED_CODE {
            return Err(SourceBoundLedgerError::InvalidKind(binding.kind()));
        }
        let depth = usize::try_from(binding.stamp.depth)
            .map_err(|_| SourceBoundLedgerError::BindingNotOpen)?;
        let metric = self
            .path_logical_metrics
            .get(depth)
            .copied()
            .ok_or(SourceBoundLedgerError::BindingNotOpen)?;
        Ok(metric)
    }

    fn validate_binding(&self, binding: BindingStamp) -> Result<(), SourceBoundLedgerError> {
        if binding.build != self.epoch.build_id() {
            return Err(SourceBoundLedgerError::WrongBindingBuild);
        }
        let depth =
            usize::try_from(binding.depth).map_err(|_| SourceBoundLedgerError::BindingNotOpen)?;
        if self.path.get(depth) != Some(&binding) {
            return Err(SourceBoundLedgerError::BindingNotOpen);
        }
        Ok(())
    }

    // Proof-harness adapter. Permit/outer affinity never enter the production
    // piece; source validation and cursor advancement happen exactly once in
    // `consume_to` below.
    #[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
    pub(crate) fn claim_to(
        &mut self,
        epoch: LiveCandidateEpoch,
        permit: FreshCoveragePermit,
        boundary: CandidateSourceBoundary,
        owner: &CandidateOpenBinding,
        part: CoveragePart,
        logical: &CandidateLogicalAction,
        affinity: GreenAffinity,
    ) -> Result<ValidatedSourceClaim, SourceBoundLedgerError> {
        if permit.build_id() != self.epoch.build_id() {
            return Err(SourceBoundLedgerError::WrongBindingBuild);
        }
        let coverage = permit.id();
        let piece = self.consume_to(epoch, boundary, owner, part, logical)?;
        Ok(ValidatedSourceClaim {
            piece,
            coverage,
            affinity,
        })
    }

    /// Production source transition. No storage identity is minted or
    /// retained at source-atom granularity.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn consume_to(
        &mut self,
        epoch: LiveCandidateEpoch,
        boundary: CandidateSourceBoundary,
        owner: &CandidateOpenBinding,
        part: CoveragePart,
        logical: &CandidateLogicalAction,
    ) -> Result<ConsumedSourcePiece, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.require_predecessor_resolved()?;
        self.validate_binding(owner.stamp)?;
        if let Some(target) = logical.target() {
            self.validate_binding(target)?;
        }
        validate_part(part)?;
        self.validate_boundary(boundary)?;
        let end = boundary.scope.absolute_end;
        if end < self.next_claim_offset {
            return Err(SourceBoundLedgerError::OutOfOrderClaim);
        }
        if end == self.next_claim_offset {
            return Err(SourceBoundLedgerError::EmptyClaim);
        }
        let metric = boundary
            .scope
            .metric_at_end
            .checked_sub(self.next_claim_metric)?;
        let special = boundary
            .scope
            .special_at_end
            .checked_sub(self.next_claim_special)?;
        match logical.kind {
            CandidateLogicalActionKind::Identity { .. } if !special.is_zero() => {
                return Err(SourceBoundLedgerError::IdentityCrossesTypedAtom);
            }
            CandidateLogicalActionKind::Atomic { source, .. }
            | CandidateLogicalActionKind::CertifiedIdentity { source, .. } => {
                if source.build != self.epoch.build_id()
                    || source.line_ordinal != self.line.ordinal
                    || source.absolute_start != self.next_claim_offset
                    || source.absolute_end != end
                {
                    return Err(SourceBoundLedgerError::WrongAtomicSource);
                }
            }
            CandidateLogicalActionKind::None
            | CandidateLogicalActionKind::Identity { .. }
            | CandidateLogicalActionKind::Hidden { .. } => {}
        }
        let piece = self.make_piece(
            self.line.ordinal,
            self.next_claim_offset,
            end,
            metric,
            owner.stamp,
            part,
            logical.retained(),
        )?;
        self.next_claim_offset = end;
        self.next_claim_metric = boundary.scope.metric_at_end;
        self.next_claim_special = boundary.scope.special_at_end;
        self.accept_piece(piece)
    }

    /// Writer-internal fast path for an ordinary terminal span whose physical
    /// owner and logical target are the same open binding. The action already
    /// carries the source-ledger-minted binding stamp, so a long identity run
    /// does not require retaining or replaying one parser-visible binding per
    /// decoded scalar.
    pub(crate) fn consume_to_logical_target(
        &mut self,
        epoch: LiveCandidateEpoch,
        boundary: CandidateSourceBoundary,
        part: CoveragePart,
        logical: CandidateLogicalAction,
    ) -> Result<ConsumedSourcePiece, SourceBoundLedgerError> {
        let target = logical.target().ok_or(SourceBoundLedgerError::Invariant(
            "ordinary replay has no logical target",
        ))?;
        let owner = CandidateOpenBinding { stamp: target };
        self.consume_to(epoch, boundary, &owner, part, &logical)
    }

    /// Binds one parser-supplied physical byte length to the exact current
    /// unclaimed prefix of the recognized physical line. No caller-provided
    /// boundary or metric enters this plan.
    pub(crate) fn mint_range_replay_plan(
        &self,
        epoch: LiveCandidateEpoch,
        owner: &CandidateOpenBinding,
        part: CoveragePart,
        physical_bytes: u64,
        recipe: CandidateWriterRangeRecipe,
    ) -> Result<CandidateRangeReplayPlan, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.require_predecessor_resolved()?;
        self.validate_binding(owner.stamp)?;
        validate_part(part)?;
        if physical_bytes == 0 {
            return Err(SourceBoundLedgerError::RangeReplayEmpty);
        }
        // A lone CR may have authoritatively read exactly one non-LF byte and
        // retained it as the next line's replay byte. That is still a precise
        // emitted boundary: the decoder validates the byte's root, offset,
        // read/cursor high-water, and the absence of partial UTF-8/CR state.
        // The range consumes that already-read byte first and cannot mint or
        // inspect any independent boundary.
        if self
            .authoritative
            .validate_emitted_line_boundary(
                self.next_claim_offset,
                self.next_claim_metric,
                self.next_claim_special,
                false,
            )
            .is_err()
            || self.line.ending_atom.is_some()
            || self.line.eof
        {
            return Err(SourceBoundLedgerError::RangeReplayUnavailable);
        }
        let ExpectedReplay::Line(recognized) = self
            .recognition
            .expected_replay
            .ok_or(SourceBoundLedgerError::RangeReplayUnavailable)?
        else {
            return Err(SourceBoundLedgerError::RangeReplayUnavailable);
        };
        if recognized.descriptor != self.descriptor
            || recognized.build != self.epoch.build_id()
            || recognized.line_ordinal != self.line.ordinal
            || recognized.absolute_start != self.line.start
        {
            return Err(SourceBoundLedgerError::RangeReplayUnavailable);
        }
        let content_end = recognized
            .absolute_end
            .checked_sub(recognized.ending.map_or(0, CandidateLineEnding::bytes))
            .ok_or(SourceBoundLedgerError::RangeReplayUnavailable)?;
        // A parser-owned physical-only command may claim the terminator as an
        // exact `None` range. Visible and hidden text recipes remain confined
        // to content, and CanonicalText therefore cannot reinterpret an EOL.
        // StageTerminator is still an independent authoritative source poll;
        // this allowance only covers explicit Consume(None, Terminal) commands.
        let maximum_end =
            if recipe == CandidateWriterRangeRecipe::None && part == CoveragePart::TERMINAL {
                recognized.absolute_end
            } else {
                content_end
            };
        let absolute_end = self
            .next_claim_offset
            .checked_add(physical_bytes)
            .ok_or(SourceBoundLedgerError::Overflow("range replay end"))?;
        if absolute_end > maximum_end {
            return Err(SourceBoundLedgerError::RangeReplayEndpointOutsideLine);
        }
        let logical = match recipe {
            CandidateWriterRangeRecipe::None => CandidateRangeReplayLogical::None,
            CandidateWriterRangeRecipe::Identity => {
                require_terminal(owner.stamp)?;
                CandidateRangeReplayLogical::Identity {
                    target: owner.stamp,
                }
            }
            CandidateWriterRangeRecipe::Hidden { affinity } => {
                require_terminal(owner.stamp)?;
                CandidateRangeReplayLogical::Hidden {
                    target: owner.stamp,
                    affinity,
                }
            }
            CandidateWriterRangeRecipe::CanonicalText => {
                require_terminal(owner.stamp)?;
                CandidateRangeReplayLogical::CanonicalText {
                    target: owner.stamp,
                }
            }
        };
        Ok(CandidateRangeReplayPlan {
            descriptor: self.descriptor,
            build: self.epoch.build_id(),
            line_ordinal: self.line.ordinal,
            absolute_start: self.next_claim_offset,
            absolute_end,
            metric_at_start: self.next_claim_metric,
            special_at_start: self.next_claim_special,
            owner: owner.stamp,
            part,
            logical,
        })
    }

    pub(crate) fn mint_remaining_identity_replay_plan(
        &self,
        epoch: LiveCandidateEpoch,
        owner: &CandidateOpenBinding,
        part: CoveragePart,
    ) -> Result<CandidateRangeReplayPlan, SourceBoundLedgerError> {
        let ExpectedReplay::Line(recognized) = self
            .recognition
            .expected_replay
            .ok_or(SourceBoundLedgerError::RangeReplayUnavailable)?
        else {
            return Err(SourceBoundLedgerError::RangeReplayUnavailable);
        };
        let content_end = recognized
            .absolute_end
            .checked_sub(recognized.ending.map_or(0, CandidateLineEnding::bytes))
            .ok_or(SourceBoundLedgerError::RangeReplayUnavailable)?;
        let physical_bytes = content_end
            .checked_sub(self.next_claim_offset)
            .ok_or(SourceBoundLedgerError::RangeReplayUnavailable)?;
        self.mint_range_replay_plan(
            epoch,
            owner,
            part,
            physical_bytes,
            CandidateWriterRangeRecipe::Identity,
        )
    }

    fn validate_range_replay_plan(
        &self,
        plan: &CandidateRangeReplayPlan,
    ) -> Result<(), SourceBoundLedgerError> {
        if plan.descriptor != self.descriptor
            || plan.build != self.epoch.build_id()
            || plan.line_ordinal != self.line.ordinal
            || plan.absolute_start < self.line.start
            || plan.absolute_start > self.next_claim_offset
            || self.next_claim_offset > plan.absolute_end
        {
            return Err(SourceBoundLedgerError::RangeReplayWrongPlan);
        }
        self.validate_binding(plan.owner)?;
        match plan.logical {
            CandidateRangeReplayLogical::None => {}
            CandidateRangeReplayLogical::Identity { target }
            | CandidateRangeReplayLogical::Hidden { target, .. }
            | CandidateRangeReplayLogical::CanonicalText { target } => {
                self.validate_binding(target)?;
            }
        }
        Ok(())
    }

    pub(crate) fn consume_range_replay_ordinary(
        &mut self,
        epoch: LiveCandidateEpoch,
        plan: &CandidateRangeReplayPlan,
        boundary: CandidateSourceBoundary,
    ) -> Result<ConsumedSourcePiece, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.validate_range_replay_plan(plan)?;
        if boundary.absolute_offset() > plan.absolute_end {
            return Err(SourceBoundLedgerError::RangeReplayEndpointSplitsAtom);
        }
        let owner = CandidateOpenBinding { stamp: plan.owner };
        let logical = match plan.logical {
            CandidateRangeReplayLogical::None => CandidateLogicalAction::none(),
            CandidateRangeReplayLogical::Identity { target }
            | CandidateRangeReplayLogical::CanonicalText { target } => CandidateLogicalAction {
                kind: CandidateLogicalActionKind::Identity { target },
            },
            CandidateRangeReplayLogical::Hidden { target, affinity } => CandidateLogicalAction {
                kind: CandidateLogicalActionKind::Hidden { target, affinity },
            },
        };
        self.consume_to(epoch, boundary, &owner, plan.part, &logical)
    }

    pub(crate) fn consume_range_replay_canonical_atom(
        &mut self,
        epoch: LiveCandidateEpoch,
        plan: &CandidateRangeReplayPlan,
        atom: &CandidateSourceAtom,
    ) -> Result<ConsumedSourcePiece, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.validate_range_replay_plan(plan)?;
        if atom.certified.absolute_end > plan.absolute_end {
            return Err(SourceBoundLedgerError::RangeReplayEndpointSplitsAtom);
        }
        let CandidateRangeReplayLogical::CanonicalText { target } = plan.logical else {
            return Err(SourceBoundLedgerError::RangeReplayUnexpectedAtom);
        };
        let owner = CandidateOpenBinding { stamp: plan.owner };
        let target = CandidateOpenBinding { stamp: target };
        let logical = match atom.kind() {
            CandidateSourceAtomKind::Tab => CandidateLogicalAction::tab_identity(&target, atom)?,
            CandidateSourceAtomKind::Nul => {
                CandidateLogicalAction::nul_to_replacement(&target, atom)?
            }
            CandidateSourceAtomKind::Scalar(_) | CandidateSourceAtomKind::LineEnding(_) => {
                return Err(SourceBoundLedgerError::RangeReplayUnexpectedAtom);
            }
        };
        self.consume_to(epoch, atom.boundary(), &owner, plan.part, &logical)
    }

    pub(crate) fn finish_range_replay(
        &self,
        epoch: LiveCandidateEpoch,
        plan: &CandidateRangeReplayPlan,
    ) -> Result<CandidateRangeReplaySourceReceipt, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.validate_range_replay_plan(plan)?;
        if self.next_claim_offset != plan.absolute_end
            || self.authoritative.emitted_offset != plan.absolute_end
            || self.authoritative.read_offset != plan.absolute_end
            || self.authoritative.decode.is_some()
            || self.authoritative.pending_cr.is_some()
            || self.authoritative.replay.is_some()
        {
            return Err(SourceBoundLedgerError::RangeReplayIncomplete);
        }
        let _special = self.next_claim_special.checked_sub(plan.special_at_start)?;
        Ok(CandidateRangeReplaySourceReceipt {
            descriptor: plan.descriptor,
            build: plan.build,
            line_ordinal: plan.line_ordinal,
            absolute_start: plan.absolute_start,
            absolute_end: plan.absolute_end,
            metric: self.next_claim_metric.checked_sub(plan.metric_at_start)?,
        })
    }

    fn validate_boundary(
        &self,
        boundary: CandidateSourceBoundary,
    ) -> Result<(), SourceBoundLedgerError> {
        if boundary.scope.build != self.epoch.build_id() {
            return Err(SourceBoundLedgerError::WrongBoundary);
        }
        if boundary.scope.line_ordinal != self.line.ordinal
            || boundary.scope.line_start != self.line.start
        {
            return Err(SourceBoundLedgerError::BoundaryFromAnotherLine);
        }
        if boundary.scope.absolute_end > self.authoritative.emitted_offset {
            return Err(SourceBoundLedgerError::WrongBoundary);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn make_piece(
        &self,
        line_ordinal: u64,
        absolute_start: u64,
        absolute_end: u64,
        metric: SourceLedgerMetric,
        owner: BindingStamp,
        part: CoveragePart,
        logical: ValidatedLogicalAction,
    ) -> Result<ConsumedSourcePiece, SourceBoundLedgerError> {
        let current_depth = u32::try_from(
            self.path
                .len()
                .checked_sub(1)
                .ok_or(SourceBoundLedgerError::BindingNotOpen)?,
        )
        .map_err(|_| SourceBoundLedgerError::Overflow("open-path depth"))?;
        let owner_relative_depth = current_depth
            .checked_sub(owner.depth)
            .ok_or(SourceBoundLedgerError::BindingNotOpen)?;
        Ok(ConsumedSourcePiece {
            descriptor: self.descriptor,
            build: self.epoch.build_id(),
            line_ordinal,
            absolute_start,
            absolute_end,
            metric,
            physical_owner: owner,
            owner_relative_depth,
            structural_state_generation: self.structural_state_generation,
            part,
            logical,
        })
    }

    fn accept_piece(
        &mut self,
        piece: ConsumedSourcePiece,
    ) -> Result<ConsumedSourcePiece, SourceBoundLedgerError> {
        let logical_update = logical_metric_update(&piece, &self.path_logical_metrics)?;
        let claim_count = self
            .claim_count
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow("claim count"))?;
        let debug_digest = piece.fold_digest(self.debug_digest);
        if let Some((depth, metric)) = logical_update {
            self.path_logical_metrics[depth] = metric;
        }
        self.claim_count = claim_count;
        self.debug_digest = debug_digest;
        Ok(piece)
    }

    // Debug staging retains a proof-harness permit beside, never inside, the
    // pending production source semantics.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn stage_terminator(
        &mut self,
        epoch: LiveCandidateEpoch,
        permit: FreshCoveragePermit,
        atom: &CandidateSourceAtom,
        terminal: &CandidateOpenBinding,
        affinity: GreenAffinity,
    ) -> Result<(), SourceBoundLedgerError> {
        if permit.build_id() != self.epoch.build_id() {
            return Err(SourceBoundLedgerError::WrongBindingBuild);
        }
        self.stage_terminator_core(epoch, Some(permit), atom, terminal, affinity)
    }

    pub(crate) fn stage_consumed_terminator(
        &mut self,
        epoch: LiveCandidateEpoch,
        atom: &CandidateSourceAtom,
        terminal: &CandidateOpenBinding,
    ) -> Result<(), SourceBoundLedgerError> {
        self.stage_terminator_core(epoch, None, atom, terminal, GreenAffinity::Downstream)
    }

    fn stage_terminator_core(
        &mut self,
        epoch: LiveCandidateEpoch,
        debug_permit: Option<FreshCoveragePermit>,
        atom: &CandidateSourceAtom,
        terminal: &CandidateOpenBinding,
        affinity: GreenAffinity,
    ) -> Result<(), SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.require_predecessor_resolved()?;
        self.validate_binding(terminal.stamp)?;
        require_terminal(terminal.stamp)?;
        let CandidateSourceAtomKind::LineEnding(_) = atom.kind() else {
            return Err(SourceBoundLedgerError::WrongAtomicSource);
        };
        let Some(ending) = self.line.ending_atom else {
            return Err(SourceBoundLedgerError::LineNotEnded);
        };
        if ending != atom.certified
            || atom.certified.absolute_start != self.next_claim_offset
            || self.pending_terminator.is_some()
        {
            return Err(SourceBoundLedgerError::PendingAlreadyStaged);
        }
        self.next_claim_offset = atom.certified.absolute_end;
        self.next_claim_metric = atom.certified.metric_after;
        self.next_claim_special = atom.certified.special_after;
        self.pending_terminator = Some(PendingTerminator {
            debug_permit,
            atom: atom.certified,
            terminal: terminal.stamp,
            affinity,
        });
        Ok(())
    }

    pub(crate) fn resolve_terminator(
        &mut self,
        epoch: LiveCandidateEpoch,
        resolution: CandidateTerminatorResolution,
    ) -> Result<ValidatedSourceClaim, SourceBoundLedgerError> {
        if self
            .pending_terminator
            .as_ref()
            .is_none_or(|pending| pending.debug_permit.is_none())
        {
            return Err(SourceBoundLedgerError::Invariant(
                "terminator was staged for production consumption",
            ));
        }
        let (piece, permit, affinity) = self.resolve_terminator_core(epoch, resolution)?;
        let permit = permit.expect("debug terminator permit was checked");
        Ok(ValidatedSourceClaim {
            piece,
            coverage: permit.id(),
            affinity,
        })
    }

    pub(crate) fn resolve_consumed_terminator(
        &mut self,
        epoch: LiveCandidateEpoch,
        resolution: CandidateTerminatorResolution,
    ) -> Result<ConsumedSourcePiece, SourceBoundLedgerError> {
        if self
            .pending_terminator
            .as_ref()
            .is_some_and(|pending| pending.debug_permit.is_some())
        {
            return Err(SourceBoundLedgerError::Invariant(
                "terminator was staged for debug consumption",
            ));
        }
        let (piece, permit, _) = self.resolve_terminator_core(epoch, resolution)?;
        debug_assert!(permit.is_none());
        Ok(piece)
    }

    fn resolve_terminator_core(
        &mut self,
        epoch: LiveCandidateEpoch,
        resolution: CandidateTerminatorResolution,
    ) -> Result<
        (
            ConsumedSourcePiece,
            Option<FreshCoveragePermit>,
            GreenAffinity,
        ),
        SourceBoundLedgerError,
    > {
        self.require_epoch(epoch)?;
        let pending = self
            .pending_terminator
            .as_ref()
            .ok_or(SourceBoundLedgerError::NoPendingTerminator)?;
        self.validate_binding(pending.terminal)?;
        let logical = match resolution {
            CandidateTerminatorResolution::CloseNone => CandidateLogicalAction::none(),
            CandidateTerminatorResolution::ContinueCanonicalNewline
            | CandidateTerminatorResolution::CloseCanonicalNewline => match pending.atom.kind {
                CandidateSourceAtomKind::LineEnding(CandidateLineEnding::Lf) => {
                    CandidateLogicalAction {
                        kind: CandidateLogicalActionKind::Identity {
                            target: pending.terminal,
                        },
                    }
                }
                CandidateSourceAtomKind::LineEnding(CandidateLineEnding::LoneCr) => {
                    CandidateLogicalAction {
                        kind: CandidateLogicalActionKind::Atomic {
                            target: pending.terminal,
                            source: pending.atom,
                            projection: CandidateAtomicProjection::LoneCrToLf,
                        },
                    }
                }
                CandidateSourceAtomKind::LineEnding(CandidateLineEnding::CrLf) => {
                    CandidateLogicalAction {
                        kind: CandidateLogicalActionKind::Atomic {
                            target: pending.terminal,
                            source: pending.atom,
                            projection: CandidateAtomicProjection::CrLfToLf,
                        },
                    }
                }
                _ => return Err(SourceBoundLedgerError::Invariant("pending terminator kind")),
            },
        };
        let part = match resolution {
            CandidateTerminatorResolution::ContinueCanonicalNewline => CoveragePart::CONTENT,
            CandidateTerminatorResolution::CloseNone
            | CandidateTerminatorResolution::CloseCanonicalNewline => CoveragePart::TERMINAL,
        };
        let CandidateSourceAtomKind::LineEnding(ending) = pending.atom.kind else {
            return Err(SourceBoundLedgerError::Invariant("pending terminator kind"));
        };
        let metric = SourceLedgerMetric {
            bytes: ending.bytes(),
            utf16: ending.bytes(),
        };
        let piece = self.make_piece(
            pending.atom.line_ordinal,
            pending.atom.absolute_start,
            pending.atom.absolute_end,
            metric,
            pending.terminal,
            part,
            logical.retained(),
        )?;
        let affinity = pending.affinity;
        let piece = self.accept_piece(piece)?;
        let pending = self
            .pending_terminator
            .take()
            .expect("pending terminator was validated");
        Ok((piece, pending.debug_permit, affinity))
    }

    pub(crate) fn stage_blank_gap(
        &mut self,
        epoch: LiveCandidateEpoch,
        permit: Option<FreshCoveragePermit>,
        affinity: GreenAffinity,
    ) -> Result<(), SourceBoundLedgerError> {
        self.stage_blank_gap_core(epoch, permit, affinity, true)
    }

    pub(crate) fn stage_consumed_blank_gap(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), SourceBoundLedgerError> {
        self.stage_blank_gap_core(epoch, None, GreenAffinity::Downstream, false)
    }

    fn stage_blank_gap_core(
        &mut self,
        epoch: LiveCandidateEpoch,
        debug_permit: Option<FreshCoveragePermit>,
        affinity: GreenAffinity,
        debug_mode: bool,
    ) -> Result<(), SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        if self.pending_terminator.is_some() {
            return Err(SourceBoundLedgerError::PreviousPendingUnresolved);
        }
        if !self.line.has_atoms || self.next_claim_offset < self.line.last_nonblank_end {
            return Err(SourceBoundLedgerError::CurrentLineIsNotBlank);
        }
        let end = self.line_physical_end()?;
        if self.next_claim_offset >= end {
            return Err(SourceBoundLedgerError::EmptyClaim);
        }
        let metric = self.metric.checked_sub(self.next_claim_metric)?;
        let gap_start = self.next_claim_offset;
        match (&mut self.pending_gap, debug_permit) {
            (None, permit) if permit.is_some() == debug_mode => {
                if permit
                    .as_ref()
                    .is_some_and(|permit| permit.build_id() != self.epoch.build_id())
                {
                    return Err(SourceBoundLedgerError::WrongBindingBuild);
                }
                self.pending_gap = Some(PendingGap {
                    debug_permit: permit,
                    start: gap_start,
                    end,
                    metric,
                    affinity,
                    first_line: self.line.ordinal,
                    binding_generation_ceiling: self.next_path_generation,
                });
            }
            (Some(gap), None) if gap.debug_permit.is_some() == debug_mode => {
                if gap.end != self.line.start || gap.affinity != affinity {
                    return Err(SourceBoundLedgerError::PendingGapAffinityMismatch);
                }
                gap.end = end;
                gap.metric = gap.metric.checked_add(metric)?;
            }
            _ => {
                return Err(SourceBoundLedgerError::Invariant("blank-gap permit shape"));
            }
        }
        self.next_claim_offset = end;
        self.next_claim_metric = self.metric;
        self.next_claim_special = self.special;
        Ok(())
    }

    pub(crate) fn resolve_gap(
        &mut self,
        epoch: LiveCandidateEpoch,
        owner: &CandidateOpenBinding,
    ) -> Result<ValidatedSourceClaim, SourceBoundLedgerError> {
        if self
            .pending_gap
            .as_ref()
            .is_none_or(|gap| gap.debug_permit.is_none())
        {
            return Err(SourceBoundLedgerError::Invariant(
                "blank gap was staged for production consumption",
            ));
        }
        let (piece, permit, affinity) = self.resolve_gap_core(epoch, owner)?;
        let permit = permit.expect("debug gap permit was checked");
        Ok(ValidatedSourceClaim {
            piece,
            coverage: permit.id(),
            affinity,
        })
    }

    pub(crate) fn resolve_consumed_gap(
        &mut self,
        epoch: LiveCandidateEpoch,
        owner: &CandidateOpenBinding,
    ) -> Result<ConsumedSourcePiece, SourceBoundLedgerError> {
        if self
            .pending_gap
            .as_ref()
            .is_some_and(|gap| gap.debug_permit.is_some())
        {
            return Err(SourceBoundLedgerError::Invariant(
                "blank gap was staged for debug consumption",
            ));
        }
        let (piece, permit, _) = self.resolve_gap_core(epoch, owner)?;
        debug_assert!(permit.is_none());
        Ok(piece)
    }

    fn resolve_gap_core(
        &mut self,
        epoch: LiveCandidateEpoch,
        owner: &CandidateOpenBinding,
    ) -> Result<
        (
            ConsumedSourcePiece,
            Option<FreshCoveragePermit>,
            GreenAffinity,
        ),
        SourceBoundLedgerError,
    > {
        self.require_epoch(epoch)?;
        self.validate_binding(owner.stamp)?;
        let gap = self
            .pending_gap
            .as_ref()
            .ok_or(SourceBoundLedgerError::NoPendingGap)?;
        if owner.stamp.path_generation >= gap.binding_generation_ceiling {
            return Err(SourceBoundLedgerError::PendingGapOwnerOpenedAfterGap);
        }
        let piece = self.make_piece(
            gap.first_line,
            gap.start,
            gap.end,
            gap.metric,
            owner.stamp,
            CoveragePart::GAP,
            CandidateLogicalAction::none().retained(),
        )?;
        let affinity = gap.affinity;
        let piece = self.accept_piece(piece)?;
        let gap = self.pending_gap.take().expect("pending gap was validated");
        Ok((piece, gap.debug_permit, affinity))
    }

    pub(crate) fn finish_line(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateLineReceipt, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.require_no_recognition_byte_session()?;
        let end = self.line_physical_end()?;
        if self.next_claim_offset != end {
            return Err(SourceBoundLedgerError::LineCoverageIncomplete);
        }
        let ending = self.line.ending_atom.and_then(|atom| match atom.kind {
            CandidateSourceAtomKind::LineEnding(value) => Some(value),
            _ => None,
        });
        let pending = if self
            .pending_terminator
            .as_ref()
            .is_some_and(|pending| pending.atom.line_ordinal == self.line.ordinal)
        {
            Some(PendingSourceKind::Terminator)
        } else if self.pending_gap.as_ref().is_some_and(|gap| gap.end == end) {
            Some(PendingSourceKind::Gap)
        } else {
            None
        };
        let mut receipt = CandidateLineReceipt {
            descriptor: self.descriptor,
            build: self.epoch.build_id(),
            line_ordinal: self.line.ordinal,
            absolute_start: self.line.start,
            absolute_end: end,
            metric: self.metric.checked_sub(self.line.metric_at_start)?,
            ending,
            pending,
            atom_count: self.line.atom_count,
            atom_debug_digest: self.line.atom_debug_digest,
            recognition_replay_matched: false,
        };
        self.reconcile_recognition_replay(&mut receipt)?;
        self.line_count = self
            .line_count
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow("line count"))?;
        let ending_bytes = receipt.ending.map_or(0, CandidateLineEnding::bytes);
        let content_bytes = receipt.metric.bytes.checked_sub(ending_bytes).ok_or(
            SourceBoundLedgerError::Invariant("line ending exceeds physical line metric"),
        )?;
        self.last_line_length = usize::try_from(content_bytes)
            .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?;
        let next_ordinal = self
            .line
            .ordinal
            .checked_add(1)
            .ok_or(SourceBoundLedgerError::Overflow("line ordinal"))?;
        self.line = LineState::new(next_ordinal, end, self.metric);
        Ok(receipt)
    }

    fn reconcile_recognition_replay(
        &mut self,
        receipt: &mut CandidateLineReceipt,
    ) -> Result<(), SourceBoundLedgerError> {
        if let Some(expected) = self.recognition.expected_replay {
            match expected {
                ExpectedReplay::Line(expected) => {
                    if expected.descriptor != receipt.descriptor
                        || expected.build != receipt.build
                        || expected.line_ordinal != receipt.line_ordinal
                        || expected.absolute_start != receipt.absolute_start
                        || expected.absolute_end != receipt.absolute_end
                        || expected.metric != receipt.metric
                        || expected.ending != receipt.ending
                        || expected.atom_count != receipt.atom_count
                        || expected.atom_debug_digest != receipt.atom_debug_digest
                    {
                        return Err(SourceBoundLedgerError::RecognitionReplayMismatch);
                    }
                    self.recognition.expected_replay = None;
                    receipt.recognition_replay_matched = true;
                }
                ExpectedReplay::Range {
                    receipt: expected,
                    mut progress,
                } => {
                    progress.line_count = progress
                        .line_count
                        .checked_add(1)
                        .ok_or(SourceBoundLedgerError::Overflow("replay range line count"))?;
                    if receipt.absolute_end > expected.absolute_end {
                        return Err(SourceBoundLedgerError::RecognitionReplayMismatch);
                    }
                    if receipt.absolute_end == expected.absolute_end {
                        let metric = self.metric.checked_sub(progress.metric_at_start)?;
                        let expected_last_line = expected
                            .line_count
                            .checked_sub(1)
                            .and_then(|additional_lines| {
                                expected.first_line.checked_add(additional_lines)
                            })
                            .ok_or(SourceBoundLedgerError::Overflow(
                                "recognition range last line",
                            ))?;
                        if expected.descriptor != receipt.descriptor
                            || expected.build != receipt.build
                            || expected_last_line != receipt.line_ordinal
                            || expected.metric != metric
                            || expected.line_count != progress.line_count
                            || expected.atom_count != progress.atom_count
                            || expected.atom_debug_digest != progress.atom_debug_digest
                        {
                            return Err(SourceBoundLedgerError::RecognitionReplayMismatch);
                        }
                        self.recognition.expected_replay = None;
                        receipt.recognition_replay_matched = true;
                    } else {
                        self.recognition.expected_replay = Some(ExpectedReplay::Range {
                            receipt: expected,
                            progress,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn line_physical_end(&self) -> Result<u64, SourceBoundLedgerError> {
        if let Some(atom) = self.line.ending_atom {
            return Ok(atom.absolute_end);
        }
        if self.line.eof && self.line.has_atoms {
            return Ok(self.authoritative.emitted_offset);
        }
        Err(SourceBoundLedgerError::LineNotEnded)
    }

    pub(crate) fn seal(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateSourceSeal, SourceBoundLedgerError> {
        self.require_epoch(epoch)?;
        self.require_not_sealed()?;
        self.require_no_recognition_byte_session()?;
        if !self.eof_observed {
            return Err(SourceBoundLedgerError::EofNotObserved);
        }
        if self.line.has_atoms {
            return Err(SourceBoundLedgerError::LineCoverageIncomplete);
        }
        self.require_predecessor_resolved()?;
        if self.recognition.expected_replay.is_some() {
            return Err(SourceBoundLedgerError::RecognitionReplayPending);
        }
        if self.next_claim_offset
            != u64::try_from(self.descriptor.bytes)
                .map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)?
            || self.authoritative.emitted_offset != self.next_claim_offset
            || self.authoritative.read_offset != self.next_claim_offset
            || self.authoritative.decode.is_some()
            || self.authoritative.pending_cr.is_some()
            || self.authoritative.replay.is_some()
        {
            return Err(SourceBoundLedgerError::LineCoverageIncomplete);
        }
        if self.metric.utf16 != self.authoritative_root_utf16 {
            return Err(SourceBoundLedgerError::RootUtf16Mismatch);
        }
        if !self.path.is_empty() || !self.path_logical_metrics.is_empty() {
            return Err(SourceBoundLedgerError::OpenBindingsAtSeal);
        }
        self.sealed = true;
        let cursor_metrics = self.authoritative.cumulative_cursor_metrics()?;
        let recognition_metrics = self.recognition.decoder.cumulative_cursor_metrics()?;
        Ok(CandidateSourceSeal {
            descriptor: self.descriptor,
            build: self.epoch.build_id(),
            metric: self.metric,
            line_count: self.line_count,
            claim_count: self.claim_count,
            debug_digest: self.debug_digest,
            maximum_decoder_bytes: self.authoritative.maximum_decoder_bytes,
            source_chunk_loads: cursor_metrics.chunk_loads,
            source_bytes_copied: cursor_metrics.chunk_bytes_copied,
            maximum_source_chunk_bytes: cursor_metrics.maximum_chunk_bytes,
            recognition_source_chunk_loads: recognition_metrics.chunk_loads,
            recognition_source_bytes_copied: recognition_metrics.chunk_bytes_copied,
            recognition_maximum_source_chunk_bytes: recognition_metrics.maximum_chunk_bytes,
            recognition_maximum_decoder_bytes: self.recognition.decoder.maximum_decoder_bytes,
            recognition_maximum_lead_bytes: self.recognition.maximum_lead_bytes,
            authoritative_root_utf16: self.authoritative_root_utf16,
            maximum_open_path_len: self.maximum_open_path_len,
            maximum_open_path_capacity_bytes: self
                .maximum_open_path_capacity
                .saturating_mul(std::mem::size_of::<BindingStamp>())
                .saturating_add(
                    self.path_logical_metrics
                        .capacity()
                        .saturating_mul(std::mem::size_of::<SourceLedgerMetric>()),
                ),
        })
    }

    fn require_epoch(&self, epoch: LiveCandidateEpoch) -> Result<(), SourceBoundLedgerError> {
        if epoch == self.epoch
            && epoch.source() == self.descriptor
            && epoch.build_id() == self.epoch.build_id()
        {
            Ok(())
        } else {
            Err(SourceBoundLedgerError::WrongEpoch)
        }
    }

    fn require_predecessor_resolved(&self) -> Result<(), SourceBoundLedgerError> {
        if self.pending_terminator.is_some() || self.pending_gap.is_some() {
            Err(SourceBoundLedgerError::PreviousPendingUnresolved)
        } else {
            Ok(())
        }
    }

    fn require_not_sealed(&self) -> Result<(), SourceBoundLedgerError> {
        if self.sealed {
            Err(SourceBoundLedgerError::AlreadySealed)
        } else {
            Ok(())
        }
    }
}

fn validate_part(part: CoveragePart) -> Result<(), SourceBoundLedgerError> {
    if matches!(
        part,
        CoveragePart::CONTENT
            | CoveragePart::CONTAINER_MARKER
            | CoveragePart::BLOCK_MARKER
            | CoveragePart::GAP
            | CoveragePart::TERMINAL
    ) {
        Ok(())
    } else {
        Err(SourceBoundLedgerError::InvalidCoveragePart(part))
    }
}

trait SourceByteOffset {
    fn offset_u64(self) -> Result<u64, SourceBoundLedgerError>;
}

impl SourceByteOffset for SourceByte {
    fn offset_u64(self) -> Result<u64, SourceBoundLedgerError> {
        u64::try_from(self.offset).map_err(|_| SourceBoundLedgerError::SourceLengthOverflow)
    }
}

fn fold_byte(digest: &mut u64, byte: u8) {
    *digest ^= u64::from(byte);
    *digest = digest.wrapping_mul(FNV_PRIME);
}

fn fold_u64(digest: &mut u64, value: u64) {
    for byte in value.to_le_bytes() {
        fold_byte(digest, byte);
    }
}

fn fold_decoded_atom(mut digest: u64, atom: DecodedAtom) -> u64 {
    fold_u64(&mut digest, atom.absolute_start);
    fold_u64(&mut digest, atom.absolute_end);
    match atom.kind {
        CandidateSourceAtomKind::Scalar(value) => {
            fold_byte(&mut digest, 1);
            fold_u64(&mut digest, u64::from(u32::from(value)));
        }
        CandidateSourceAtomKind::Tab => fold_byte(&mut digest, 2),
        CandidateSourceAtomKind::Nul => fold_byte(&mut digest, 3),
        CandidateSourceAtomKind::LineEnding(CandidateLineEnding::Lf) => {
            fold_byte(&mut digest, 4);
        }
        CandidateSourceAtomKind::LineEnding(CandidateLineEnding::LoneCr) => {
            fold_byte(&mut digest, 5);
        }
        CandidateSourceAtomKind::LineEnding(CandidateLineEnding::CrLf) => {
            fold_byte(&mut digest, 6);
        }
    }
    digest
}
