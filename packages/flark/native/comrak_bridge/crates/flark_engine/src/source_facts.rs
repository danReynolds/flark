//! Resumable, parser-independent facts over one immutable source lease.
//!
//! Crop remains the exact text replica. This module builds the one discardable
//! measured index that later parser certification and publication bind. A
//! content fingerprint is a corruption/convergence guard; it never replaces
//! the exact [`SourceVersion`] and lease lineage carried by every result.

use std::collections::LinkedList;
use std::fmt;
use std::mem::size_of;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::measured_sequence::{
    begin_measured_sequence_seal, retain_committed_measured_sequence_root,
    splice_measured_sequence_atomic, CommittedMeasuredSequenceRoot,
    CommittedMeasuredSequenceRootReleaseFailure, MeasuredSequenceBuildRoot, MeasuredSequenceSeal,
    ResumableMeasuredSequenceBuilder, ResumableSequenceProgress, SequenceInspectionReceipt,
    SequenceMeasure, SequenceMutationReceipt, SequenceSpec, SequenceSpecInspection,
};
use crate::source::{SourceCursor, SourceEditError, SourceSnapshotLease, SourceVersion};
use crate::storage::{CandidateBuild, PageArena};
use crate::{ArenaError, ArenaId, ARENA_PAGE_BYTES, SOURCE_CURSOR_WINDOW_BYTES};

/// Version of the rolling fingerprint shared with the Dart source owner.
pub const SOURCE_CONTENT_FINGERPRINT_ALGORITHM: u32 = 1;
/// Maximum UTF-16 distance between persisted source checkpoints.
pub const SOURCE_FACT_CHECKPOINT_SPACING_MAX_UTF16: usize = 4 * 1024;
/// Maximum checkpoint records returned by one poll.
pub const SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX: usize = 64;
/// Domain/version for the shape-independent persistent checkpoint-root guard.
///
/// This is intentionally distinct from the legacy absolute-checkpoint fold
/// used by the clean session publication. It authenticates the persistent
/// relative-page sequence that incremental splices actually reuse.
pub const PERSISTENT_SOURCE_FACTS_CHECKPOINT_ROOT_GUARD_ALGORITHM: u32 = 2;
/// Maximum source bytes one poll may inspect, independent of caller honesty.
pub const SOURCE_FACT_SOURCE_BYTES_PER_POLL_MAX: usize = 64 * 1024;
/// Bounded production default for one completed source-fact root.
pub const SOURCE_FACT_ROOT_DEFAULT_MAX_CHECKPOINTS: usize = 1_000_000;
/// Bounded production default for canonical checkpoint pages.
pub const SOURCE_FACT_ROOT_DEFAULT_MAX_PAGES: usize =
    SOURCE_FACT_ROOT_DEFAULT_MAX_CHECKPOINTS.div_ceil(SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX);
/// Conservative bounded production residency for checkpoint records and page metadata.
pub const SOURCE_FACT_ROOT_DEFAULT_MAX_RESIDENT_BYTES: usize = 128 * 1024 * 1024;

const HASH_BASES: [u32; 4] = [0x0010_0193, 0x9e37_79b1, 0x85eb_ca77, 0xc2b2_ae3d];
static NEXT_SOURCE_FACT_SCAN_ID: AtomicU64 = AtomicU64::new(1);

/// Versioned parameters that determine one canonical source-fact scan.
///
/// The content-fingerprint algorithm is fixed by this crate version. Keeping
/// it in the profile makes unsupported or crossed-version completions fail
/// closed instead of silently certifying a differently interpreted stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFactsScanProfile {
    checkpoint_spacing_utf16: u64,
    content_fingerprint_algorithm: u32,
}

/// Stable, nonzero identity for the configured markdown grammar/syntax policy.
///
/// Source facts are grammar-independent, but structural publication is not.
/// Binding this identity at certification prevents facts scanned for one exact
/// source from authorizing a parser running under a different profile.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ParserProfileId(NonZeroU64);

impl ParserProfileId {
    /// Creates a parser profile identity. Zero is reserved as "unbound".
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl SourceFactsScanProfile {
    /// Creates the currently supported scan profile.
    pub fn new(checkpoint_spacing_utf16: usize) -> Result<Self, SourceFactsError> {
        if !(2..=SOURCE_FACT_CHECKPOINT_SPACING_MAX_UTF16).contains(&checkpoint_spacing_utf16) {
            return Err(SourceFactsError::InvalidCheckpointSpacing);
        }
        Ok(Self {
            checkpoint_spacing_utf16: u64::try_from(checkpoint_spacing_utf16)
                .map_err(|_| SourceFactsError::InvalidCheckpointSpacing)?,
            content_fingerprint_algorithm: SOURCE_CONTENT_FINGERPRINT_ALGORITHM,
        })
    }

    /// Returns the maximum intended UTF-16 distance between checkpoints.
    #[must_use]
    pub const fn checkpoint_spacing_utf16(self) -> u64 {
        self.checkpoint_spacing_utf16
    }

    /// Returns the required version of the rolling content guard.
    #[must_use]
    pub const fn content_fingerprint_algorithm(self) -> u32 {
        self.content_fingerprint_algorithm
    }
}

/// Explicit admission bounds for one assembled source-fact root.
///
/// These limits are checked against the exact source dimensions before the
/// builder accepts any transport page, and checked again while assembling.
/// The bounded [`Default`] admits a 100 MiB ASCII source at the production
/// 4096-UTF-16 checkpoint spacing while rejecting impractically dense roots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFactsRootLimits {
    max_checkpoints: usize,
    max_pages: usize,
    max_resident_bytes: usize,
}

impl SourceFactsRootLimits {
    /// Creates explicit nonzero root limits.
    pub const fn new(
        max_checkpoints: usize,
        max_pages: usize,
        max_resident_bytes: usize,
    ) -> Option<Self> {
        if max_checkpoints == 0 || max_pages == 0 || max_resident_bytes == 0 {
            None
        } else {
            Some(Self {
                max_checkpoints,
                max_pages,
                max_resident_bytes,
            })
        }
    }

    #[must_use]
    pub const fn max_checkpoints(self) -> usize {
        self.max_checkpoints
    }

    #[must_use]
    pub const fn max_pages(self) -> usize {
        self.max_pages
    }

    #[must_use]
    pub const fn max_resident_bytes(self) -> usize {
        self.max_resident_bytes
    }
}

impl Default for SourceFactsRootLimits {
    fn default() -> Self {
        Self {
            max_checkpoints: SOURCE_FACT_ROOT_DEFAULT_MAX_CHECKPOINTS,
            max_pages: SOURCE_FACT_ROOT_DEFAULT_MAX_PAGES,
            max_resident_bytes: SOURCE_FACT_ROOT_DEFAULT_MAX_RESIDENT_BYTES,
        }
    }
}

/// Conservative preflight metrics for one canonical source-fact root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFactsRootAdmission {
    checkpoint_count: usize,
    page_count: usize,
    resident_bytes: usize,
}

impl SourceFactsRootAdmission {
    /// Computes and validates the root's conservative worst-case residency
    /// before any checkpoint page is accumulated.
    pub fn for_source(
        source: SourceVersion,
        profile: SourceFactsScanProfile,
        limits: SourceFactsRootLimits,
    ) -> Result<Self, SourceFactsAssemblyError> {
        Self::for_utf16_len(source.utf16_len(), profile, limits)
    }

    fn for_utf16_len(
        source_utf16_len: usize,
        profile: SourceFactsScanProfile,
        limits: SourceFactsRootLimits,
    ) -> Result<Self, SourceFactsAssemblyError> {
        validate_scan_profile(profile).map_err(|_| SourceFactsAssemblyError::InvalidProfile)?;
        if SourceFactsRootLimits::new(
            limits.max_checkpoints,
            limits.max_pages,
            limits.max_resident_bytes,
        )
        .is_none()
        {
            return Err(SourceFactsAssemblyError::InvalidLimits);
        }
        let spacing = usize::try_from(profile.checkpoint_spacing_utf16)
            .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?;
        let checkpoint_count = source_utf16_len.div_ceil(spacing);
        let page_count = checkpoint_count.div_ceil(SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX);
        let resident_bytes = source_facts_resident_bytes(checkpoint_count, page_count)?;
        let admission = Self {
            checkpoint_count,
            page_count,
            resident_bytes,
        };
        admission.enforce(limits)?;
        Ok(admission)
    }

    fn enforce(self, limits: SourceFactsRootLimits) -> Result<(), SourceFactsAssemblyError> {
        if self.checkpoint_count > limits.max_checkpoints {
            return Err(SourceFactsAssemblyError::AdmissionCheckpointLimitExceeded {
                needed: self.checkpoint_count,
                limit: limits.max_checkpoints,
            });
        }
        if self.page_count > limits.max_pages {
            return Err(SourceFactsAssemblyError::AdmissionPageLimitExceeded {
                needed: self.page_count,
                limit: limits.max_pages,
            });
        }
        if self.resident_bytes > limits.max_resident_bytes {
            return Err(
                SourceFactsAssemblyError::AdmissionResidentBytesLimitExceeded {
                    needed: self.resident_bytes,
                    limit: limits.max_resident_bytes,
                },
            );
        }
        Ok(())
    }

    #[must_use]
    pub const fn checkpoint_count(self) -> usize {
        self.checkpoint_count
    }

    #[must_use]
    pub const fn page_count(self) -> usize {
        self.page_count
    }

    #[must_use]
    pub const fn resident_bytes(self) -> usize {
        self.resident_bytes
    }
}

fn source_facts_resident_bytes(
    checkpoint_count: usize,
    page_count: usize,
) -> Result<usize, SourceFactsAssemblyError> {
    let checkpoint_bytes = checkpoint_count
        .checked_mul(
            size_of::<SourceFactCheckpoint>()
                .checked_add(size_of::<SourceFactRelativeCheckpoint>())
                .ok_or(SourceFactsAssemblyError::CounterExhausted)?,
        )
        .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
    // Includes the page object, Arc counters/pointer, and linked-list links.
    // The estimate deliberately errs high and excludes shared source storage,
    // which is charged by the runtime's independent source-retirement budget.
    let page_metadata_bytes = page_count
        .checked_mul(
            size_of::<SourceFactRootPage>()
                .checked_add(size_of::<SourceFactCanonicalPage>())
                .and_then(|bytes| bytes.checked_add(size_of::<Arc<SourceFactCanonicalPage>>()))
                .and_then(|bytes| bytes.checked_add(size_of::<Arc<SourceFactRootPage>>()))
                .and_then(|bytes| bytes.checked_add(4 * size_of::<usize>()))
                .ok_or(SourceFactsAssemblyError::CounterExhausted)?,
        )
        .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
    checkpoint_bytes
        .checked_add(page_metadata_bytes)
        .and_then(|bytes| bytes.checked_add(size_of::<SourceFactsRoot>()))
        .and_then(|bytes| bytes.checked_add(size_of::<SourceFactsRootBuilder>()))
        .ok_or(SourceFactsAssemblyError::CounterExhausted)
}

/// Distinguishes concurrent or restarted scans over the same source version.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SourceFactsScanId(u64);

impl SourceFactsScanId {
    fn allocate() -> Result<Self, SourceFactsError> {
        NEXT_SOURCE_FACT_SCAN_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map(Self)
            .map_err(|_| SourceFactsError::CounterExhausted)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Digest of the ordered checkpoint stream, independent of poll page cuts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFactSequenceDigest([u8; 32]);

impl SourceFactSequenceDigest {
    #[must_use]
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Four independently combined wrapping-`u32` polynomial lanes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceContentHash128 {
    words: [u32; 4],
}

impl SourceContentHash128 {
    /// Returns the four words in wire order.
    #[must_use]
    pub const fn words(self) -> [u32; 4] {
        self.words
    }

    fn append_byte(&mut self, byte: u8) {
        let value = u32::from(byte) + 1;
        for (word, base) in self.words.iter_mut().zip(HASH_BASES) {
            *word = word.wrapping_mul(base).wrapping_add(value);
        }
    }

    fn suffix_after(self, prefix: Self, suffix_byte_len: u64) -> Self {
        let mut words = [0_u32; 4];
        for (((output, whole), prefix), base) in words
            .iter_mut()
            .zip(self.words)
            .zip(prefix.words)
            .zip(HASH_BASES)
        {
            *output = whole.wrapping_sub(prefix.wrapping_mul(wrapping_pow(base, suffix_byte_len)));
        }
        Self { words }
    }

    fn followed_by(self, suffix: Self, suffix_byte_len: u64) -> Self {
        let mut words = [0_u32; 4];
        for (((output, prefix), suffix), base) in words
            .iter_mut()
            .zip(self.words)
            .zip(suffix.words)
            .zip(HASH_BASES)
        {
            *output = prefix
                .wrapping_mul(wrapping_pow(base, suffix_byte_len))
                .wrapping_add(suffix);
        }
        Self { words }
    }
}

fn wrapping_pow(mut base: u32, mut exponent: u64) -> u32 {
    let mut value = 1_u32;
    while exponent > 0 {
        if exponent & 1 == 1 {
            value = value.wrapping_mul(base);
        }
        base = base.wrapping_mul(base);
        exponent >>= 1;
    }
    value
}

/// Relative facts for one contiguous UTF-8 source segment.
///
/// Unlike a document-prefix checkpoint, this value is independent of where
/// the segment is installed. `checked_followed_by` reconstructs exact totals
/// without reading either segment. The two boundary bits are sufficient to
/// fold a CR/LF split across the join into one logical line ending.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceFactSegmentSummary {
    byte_len: u64,
    utf16_len: u64,
    logical_line_breaks: u64,
    rolling_hash: SourceContentHash128,
    starts_with_line_feed: bool,
    ends_with_carriage_return: bool,
}

impl SourceFactSegmentSummary {
    /// Returns the UTF-8 byte length of this segment.
    #[must_use]
    pub const fn byte_len(self) -> u64 {
        self.byte_len
    }

    /// Returns the UTF-16 code-unit length of this segment.
    #[must_use]
    pub const fn utf16_len(self) -> u64 {
        self.utf16_len
    }

    /// Returns line endings when this segment is interpreted in isolation.
    #[must_use]
    pub const fn logical_line_breaks(self) -> u64 {
        self.logical_line_breaks
    }

    /// Returns the segment-local rolling content guard.
    #[must_use]
    pub const fn rolling_hash(self) -> SourceContentHash128 {
        self.rolling_hash
    }

    /// Returns whether the first byte is LF.
    #[must_use]
    pub const fn starts_with_line_feed(self) -> bool {
        self.starts_with_line_feed
    }

    /// Returns whether the last byte is CR.
    #[must_use]
    pub const fn ends_with_carriage_return(self) -> bool {
        self.ends_with_carriage_return
    }

    /// Composes adjacent segment summaries, failing closed on metric overflow.
    #[must_use]
    pub fn checked_followed_by(self, suffix: Self) -> Option<Self> {
        if !self.is_structurally_valid() || !suffix.is_structurally_valid() {
            return None;
        }
        let byte_len = self.byte_len.checked_add(suffix.byte_len)?;
        let utf16_len = self.utf16_len.checked_add(suffix.utf16_len)?;
        let joined_crlf = u64::from(self.ends_with_carriage_return && suffix.starts_with_line_feed);
        let logical_line_breaks = self
            .logical_line_breaks
            .checked_add(suffix.logical_line_breaks)?
            .checked_sub(joined_crlf)?;
        let starts_with_line_feed = if self.byte_len == 0 {
            suffix.starts_with_line_feed
        } else {
            self.starts_with_line_feed
        };
        let ends_with_carriage_return = if suffix.byte_len == 0 {
            self.ends_with_carriage_return
        } else {
            suffix.ends_with_carriage_return
        };
        Some(Self {
            byte_len,
            utf16_len,
            logical_line_breaks,
            rolling_hash: self
                .rolling_hash
                .followed_by(suffix.rolling_hash, suffix.byte_len),
            starts_with_line_feed,
            ends_with_carriage_return,
        })
    }

    fn is_structurally_valid(self) -> bool {
        if self.byte_len == 0 {
            return self.utf16_len == 0
                && self.logical_line_breaks == 0
                && self.rolling_hash == SourceContentHash128::default()
                && !self.starts_with_line_feed
                && !self.ends_with_carriage_return;
        }
        self.utf16_len > 0
            && self.utf16_len <= self.byte_len
            && self.logical_line_breaks <= self.utf16_len
    }
}

/// Shape-independent content guard for one complete UTF-8 source stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceContentFingerprint {
    algorithm: u32,
    byte_len: u64,
    utf16_len: u64,
    rolling_hash: SourceContentHash128,
}

impl SourceContentFingerprint {
    #[must_use]
    pub const fn algorithm(self) -> u32 {
        self.algorithm
    }

    #[must_use]
    pub const fn byte_len(self) -> u64 {
        self.byte_len
    }

    #[must_use]
    pub const fn utf16_len(self) -> u64 {
        self.utf16_len
    }

    #[must_use]
    pub const fn rolling_hash(self) -> SourceContentHash128 {
        self.rolling_hash
    }
}

/// One prefix summary emitted at a scalar- and CRLF-safe boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceFactCheckpoint {
    byte_offset: u64,
    utf16_offset: u64,
    logical_line_breaks: u64,
    rolling_hash: SourceContentHash128,
}

/// One page-local checkpoint expressed from the beginning of its canonical
/// page rather than from the beginning of the document.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceFactRelativeCheckpoint {
    summary: SourceFactSegmentSummary,
}

impl SourceFactRelativeCheckpoint {
    #[must_use]
    pub const fn summary(self) -> SourceFactSegmentSummary {
        self.summary
    }
}

impl SourceFactCheckpoint {
    #[must_use]
    pub const fn byte_offset(self) -> u64 {
        self.byte_offset
    }

    #[must_use]
    pub const fn utf16_offset(self) -> u64 {
        self.utf16_offset
    }

    #[must_use]
    pub const fn logical_line_breaks(self) -> u64 {
        self.logical_line_breaks
    }

    #[must_use]
    pub const fn rolling_hash(self) -> SourceContentHash128 {
        self.rolling_hash
    }
}

/// One bounded page of monotonically increasing source-prefix facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFactCheckpointPage {
    scan_id: SourceFactsScanId,
    source: SourceVersion,
    checkpoint_spacing_utf16: u64,
    ordinal: u64,
    sequence_digest: SourceFactSequenceDigest,
    checkpoints: Box<[SourceFactCheckpoint]>,
}

impl SourceFactCheckpointPage {
    #[must_use]
    pub const fn scan_id(&self) -> SourceFactsScanId {
        self.scan_id
    }

    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    #[must_use]
    pub const fn checkpoint_spacing_utf16(&self) -> u64 {
        self.checkpoint_spacing_utf16
    }

    /// Returns the ordered-stream digest through this page's last record.
    #[must_use]
    pub const fn sequence_digest(&self) -> SourceFactSequenceDigest {
        self.sequence_digest
    }

    #[must_use]
    pub fn checkpoints(&self) -> &[SourceFactCheckpoint] {
        &self.checkpoints
    }
}

/// Complete facts for exactly one immutable source lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFactsCompletion {
    scan_id: SourceFactsScanId,
    source: SourceVersion,
    checkpoint_spacing_utf16: u64,
    checkpoint_sequence_digest: SourceFactSequenceDigest,
    fingerprint: SourceContentFingerprint,
    logical_line_breaks: u64,
    checkpoint_count: u64,
    page_count: u64,
}

impl SourceFactsCompletion {
    #[must_use]
    pub const fn scan_id(self) -> SourceFactsScanId {
        self.scan_id
    }

    #[must_use]
    pub const fn source(self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn fingerprint(self) -> SourceContentFingerprint {
        self.fingerprint
    }

    #[must_use]
    pub const fn checkpoint_spacing_utf16(self) -> u64 {
        self.checkpoint_spacing_utf16
    }

    #[must_use]
    pub const fn checkpoint_sequence_digest(self) -> SourceFactSequenceDigest {
        self.checkpoint_sequence_digest
    }

    #[must_use]
    pub const fn logical_line_breaks(self) -> u64 {
        self.logical_line_breaks
    }

    #[must_use]
    pub const fn checkpoint_count(self) -> u64 {
        self.checkpoint_count
    }

    #[must_use]
    pub const fn page_count(self) -> u64 {
        self.page_count
    }
}

/// Work performed by one bounded source-fact poll.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceFactsWork {
    source_bytes_examined: usize,
    source_bytes_buffered: usize,
    cursor_refills: usize,
    cursor_copy_bytes_upper_bound: usize,
    checkpoints_emitted: usize,
}

impl SourceFactsWork {
    #[must_use]
    pub const fn source_bytes_examined(self) -> usize {
        self.source_bytes_examined
    }

    /// Bytes copied from the scanner-owned Crop cursor into its fixed buffer.
    #[must_use]
    pub const fn source_bytes_buffered(self) -> usize {
        self.source_bytes_buffered
    }

    #[must_use]
    pub const fn cursor_refills(self) -> usize {
        self.cursor_refills
    }

    /// Conservative charge for Crop's hidden fixed-window refill copies.
    #[must_use]
    pub const fn cursor_copy_bytes_upper_bound(self) -> usize {
        self.cursor_copy_bytes_upper_bound
    }

    #[must_use]
    pub const fn checkpoints_emitted(self) -> usize {
        self.checkpoints_emitted
    }
}

/// Result of one bounded scan poll.
#[derive(Debug, Eq, PartialEq)]
pub enum SourceFactsPoll {
    Pending(SourceFactsWork),
    Page {
        page: SourceFactCheckpointPage,
        work: SourceFactsWork,
    },
    Complete {
        completion: SourceFactsCompletion,
        work: SourceFactsWork,
    },
    Cancelled,
}

/// Failure while constructing source facts.
#[derive(Debug, Eq, PartialEq)]
pub enum SourceFactsError {
    InvalidCheckpointSpacing,
    UnsupportedFingerprintAlgorithm,
    ZeroFuel,
    PollLimitExceeded,
    ScannerPoisoned,
    AllocationFailed,
    Source(SourceEditError),
    CorruptSource(&'static str),
    CounterExhausted,
    AlreadyComplete,
}

impl fmt::Display for SourceFactsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCheckpointSpacing => {
                formatter.write_str("invalid source-fact checkpoint spacing")
            }
            Self::UnsupportedFingerprintAlgorithm => {
                formatter.write_str("unsupported source-fact fingerprint algorithm")
            }
            Self::ZeroFuel => formatter.write_str("source-fact poll requires nonzero fuel"),
            Self::PollLimitExceeded => formatter.write_str("source-fact poll exceeds hard limits"),
            Self::ScannerPoisoned => formatter.write_str("source-fact scanner is poisoned"),
            Self::AllocationFailed => formatter.write_str("source-fact allocation failed"),
            Self::Source(error) => write!(formatter, "source-fact cursor failed: {error}"),
            Self::CorruptSource(message) => write!(formatter, "corrupt source: {message}"),
            Self::CounterExhausted => formatter.write_str("source-fact counter exhausted"),
            Self::AlreadyComplete => formatter.write_str("source-fact scanner already completed"),
        }
    }
}

impl std::error::Error for SourceFactsError {}

impl From<SourceEditError> for SourceFactsError {
    fn from(error: SourceEditError) -> Self {
        Self::Source(error)
    }
}

fn validate_scan_profile(profile: SourceFactsScanProfile) -> Result<(), SourceFactsError> {
    let maximum_spacing = u64::try_from(SOURCE_FACT_CHECKPOINT_SPACING_MAX_UTF16)
        .map_err(|_| SourceFactsError::InvalidCheckpointSpacing)?;
    if !(2..=maximum_spacing).contains(&profile.checkpoint_spacing_utf16) {
        return Err(SourceFactsError::InvalidCheckpointSpacing);
    }
    if profile.content_fingerprint_algorithm != SOURCE_CONTENT_FINGERPRINT_ALGORITHM {
        return Err(SourceFactsError::UnsupportedFingerprintAlgorithm);
    }
    Ok(())
}

/// A malformed or crossed source-fact stream cannot mint a certified source.
#[derive(Debug, Eq, PartialEq)]
pub enum SourceFactsAssemblyError {
    InvalidProfile,
    InvalidLimits,
    AdmissionCheckpointLimitExceeded { needed: usize, limit: usize },
    AdmissionPageLimitExceeded { needed: usize, limit: usize },
    AdmissionResidentBytesLimitExceeded { needed: usize, limit: usize },
    BuilderPoisoned,
    SourceMismatch,
    ProfileMismatch,
    ScanMismatch,
    UnexpectedPageOrdinal { expected: u64, actual: u64 },
    EmptyCheckpointPage,
    CheckpointPageTooLarge { observed: usize, limit: usize },
    CheckpointOutOfBounds,
    CheckpointNotMonotonic,
    CheckpointCoverageGap,
    CheckpointCoordinateMismatch,
    LogicalLineBreakRegression,
    TerminalCheckpointNotLast,
    PageAfterTerminalCheckpoint,
    PageDigestMismatch,
    PageCountMismatch,
    CheckpointCountMismatch,
    CompletionDigestMismatch,
    FingerprintAlgorithmMismatch,
    SourceDimensionMismatch,
    MissingTerminalCheckpoint,
    TerminalFingerprintMismatch,
    TerminalLineBreakMismatch,
    EmptySourceFactsMismatch,
    PageBoundaryReadFailed,
    CanonicalSummaryMismatch,
    CorruptPersistentSequence(&'static str),
    Arena(ArenaError),
    CounterExhausted,
    AllocationFailed,
}

impl fmt::Display for SourceFactsAssemblyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfile => formatter.write_str("invalid source-fact assembly profile"),
            Self::InvalidLimits => formatter.write_str("invalid source-fact root limits"),
            Self::AdmissionCheckpointLimitExceeded { needed, limit } => write!(
                formatter,
                "source-fact root needs {needed} checkpoints but the limit is {limit}"
            ),
            Self::AdmissionPageLimitExceeded { needed, limit } => write!(
                formatter,
                "source-fact root needs {needed} pages but the limit is {limit}"
            ),
            Self::AdmissionResidentBytesLimitExceeded { needed, limit } => write!(
                formatter,
                "source-fact root needs {needed} resident bytes but the limit is {limit}"
            ),
            Self::BuilderPoisoned => formatter.write_str("source-fact root builder is poisoned"),
            Self::SourceMismatch => formatter.write_str("source-fact source authority mismatch"),
            Self::ProfileMismatch => formatter.write_str("source-fact scan profile mismatch"),
            Self::ScanMismatch => formatter.write_str("source-fact scan lineage mismatch"),
            Self::UnexpectedPageOrdinal { expected, actual } => write!(
                formatter,
                "source-fact page ordinal {actual} does not continue {expected}"
            ),
            Self::EmptyCheckpointPage => formatter.write_str("source-fact page is empty"),
            Self::CheckpointPageTooLarge { observed, limit } => write!(
                formatter,
                "source-fact page has {observed} checkpoints but the limit is {limit}"
            ),
            Self::CheckpointOutOfBounds => {
                formatter.write_str("source-fact checkpoint is outside the exact source")
            }
            Self::CheckpointNotMonotonic => {
                formatter.write_str("source-fact checkpoints are not strictly monotonic")
            }
            Self::CheckpointCoverageGap => {
                formatter.write_str("source-fact checkpoint coverage has a gap")
            }
            Self::CheckpointCoordinateMismatch => {
                formatter.write_str("source-fact UTF-8 and UTF-16 checkpoint coordinates disagree")
            }
            Self::LogicalLineBreakRegression => {
                formatter.write_str("source-fact logical line count regressed")
            }
            Self::TerminalCheckpointNotLast => {
                formatter.write_str("terminal source-fact checkpoint is not last in its page")
            }
            Self::PageAfterTerminalCheckpoint => {
                formatter.write_str("source-fact page followed terminal coverage")
            }
            Self::PageDigestMismatch => {
                formatter.write_str("source-fact page sequence digest mismatch")
            }
            Self::PageCountMismatch => formatter.write_str("source-fact page count mismatch"),
            Self::CheckpointCountMismatch => {
                formatter.write_str("source-fact checkpoint count mismatch")
            }
            Self::CompletionDigestMismatch => {
                formatter.write_str("source-fact completion digest mismatch")
            }
            Self::FingerprintAlgorithmMismatch => {
                formatter.write_str("source-fact fingerprint algorithm mismatch")
            }
            Self::SourceDimensionMismatch => {
                formatter.write_str("source-fact terminal dimensions mismatch")
            }
            Self::MissingTerminalCheckpoint => {
                formatter.write_str("source-fact stream lacks complete terminal coverage")
            }
            Self::TerminalFingerprintMismatch => {
                formatter.write_str("source-fact terminal fingerprint mismatch")
            }
            Self::TerminalLineBreakMismatch => {
                formatter.write_str("source-fact terminal line count mismatch")
            }
            Self::EmptySourceFactsMismatch => {
                formatter.write_str("empty source-fact completion is not canonical")
            }
            Self::PageBoundaryReadFailed => {
                formatter.write_str("source-fact canonical page boundary could not be read")
            }
            Self::CanonicalSummaryMismatch => {
                formatter.write_str("source-fact canonical page summaries do not match completion")
            }
            Self::CorruptPersistentSequence(message) => {
                write!(
                    formatter,
                    "corrupt persistent source-fact sequence: {message}"
                )
            }
            Self::Arena(error) => write!(formatter, "source-fact arena failure: {error}"),
            Self::CounterExhausted => formatter.write_str("source-fact assembly counter exhausted"),
            Self::AllocationFailed => formatter.write_str("source-fact root allocation failed"),
        }
    }
}

impl std::error::Error for SourceFactsAssemblyError {}

impl From<ArenaError> for SourceFactsAssemblyError {
    fn from(error: ArenaError) -> Self {
        Self::Arena(error)
    }
}

/// Proof kind carried by [`CertifiedSource`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceFactsCoverage {
    /// One exact immutable lease was scanned contiguously through clean EOF.
    CleanEof,
    /// One exact scalar-aligned byte range was scanned contiguously under the
    /// full immutable source authority. This capability may build a splice
    /// replacement but cannot authorize whole-document parsing.
    ExactRange { byte_start: u64, byte_end: u64 },
}

/// Content digest of one bounded checkpoint page, excluding scan lineage,
/// source-root identity, revision, and page ordinal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFactPageDigest([u8; 32]);

impl SourceFactPageDigest {
    #[must_use]
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Reusable content of one immutable, bounded SourceFacts page.
///
/// Its digest and checkpoints are entirely page-local. Source identity,
/// revision, page ordinal, and document-prefix coordinates belong to a fresh
/// root binding, so this object can be shared by identity after authenticated
/// convergence.
#[derive(Debug)]
pub struct SourceFactCanonicalPage {
    content_digest: SourceFactPageDigest,
    summary: SourceFactSegmentSummary,
    checkpoints: Box<[SourceFactRelativeCheckpoint]>,
}

impl SourceFactCanonicalPage {
    #[must_use]
    pub const fn content_digest(&self) -> SourceFactPageDigest {
        self.content_digest
    }

    #[must_use]
    pub const fn summary(&self) -> SourceFactSegmentSummary {
        self.summary
    }

    #[must_use]
    pub fn checkpoints(&self) -> &[SourceFactRelativeCheckpoint] {
        &self.checkpoints
    }
}

mod persistent_sequence_codec {
    use super::*;

    const SOURCE_FACT_SEQUENCE_SCHEMA: u32 = 2;
    const SOURCE_FACT_SEQUENCE_LEAF_MAGIC: [u8; 4] = *b"SFL2";
    const SOURCE_FACT_SEQUENCE_BRANCH_MAGIC: [u8; 4] = *b"SFB2";
    pub(super) const SOURCE_FACT_SEGMENT_ENCODED_BYTES: usize = 8 + 8 + 8 + 16 + 1;
    pub(super) const SOURCE_FACT_SEQUENCE_LEAF_FIXED_BYTES: usize =
        4 + 4 + 8 + 4 + 2 + 32 + SOURCE_FACT_SEGMENT_ENCODED_BYTES;
    const SOURCE_FACT_SEQUENCE_COMMITMENT_LANES: usize = 5;
    const SOURCE_FACT_SEQUENCE_COMMITMENT_BYTES: usize =
        SOURCE_FACT_SEQUENCE_COMMITMENT_LANES * 2 * 8;
    const SOURCE_FACT_SEQUENCE_BRANCH_BYTES: usize = 4
        + 4
        + 8
        + 2
        + 8
        + 4
        + 8
        + SOURCE_FACT_SEGMENT_ENCODED_BYTES
        + SOURCE_FACT_SEQUENCE_COMMITMENT_BYTES;

    // Ordered, shape-independent concatenation commitment. Each exact leaf is
    // first bound by one domain-separated BLAKE3 XOF whose disjoint 64-bit
    // outputs supply five coefficients, then evaluated in five polynomial
    // lanes over the Mersenne prime 2^61 - 1.
    // `(value, power)` is a monoid:
    // `(L || R).value = L.value * R.power + R.value`. Consequently AVL
    // rotations and splice history cannot change the result for equal leaves.
    //
    // Security model: BLAKE3 is treated as a random oracle for coefficients;
    // the public polynomial is an algebraic accumulator, not independently a
    // collision-resistant hash. Five distinct evaluations expose 305 field
    // bits, while the final domain-separated BLAKE3 role digest caps the claim
    // at conventional 128-bit collision security.
    pub(super) const COMMITMENT_MODULUS: u64 = crate::mersenne61::MODULUS;
    pub(super) const COMMITMENT_BASES: [u64; SOURCE_FACT_SEQUENCE_COMMITMENT_LANES] = [
        0x0a09_e667_f3bc_c909,
        0x1b67_ae85_84ca_a73b,
        0x1c6e_f372_fe94_f82b,
        0x154f_f53a_5f1d_36f1,
        0x110e_527f_ade6_82d1,
    ];

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct SourceFactsSequenceCommitment {
        pub(super) values: [u64; SOURCE_FACT_SEQUENCE_COMMITMENT_LANES],
        pub(super) powers: [u64; SOURCE_FACT_SEQUENCE_COMMITMENT_LANES],
    }

    impl SourceFactsSequenceCommitment {
        pub(super) const fn empty() -> Self {
            Self {
                values: [0; SOURCE_FACT_SEQUENCE_COMMITMENT_LANES],
                powers: [1; SOURCE_FACT_SEQUENCE_COMMITMENT_LANES],
            }
        }
    }

    /// Associative semantic measure stored by the persistent SourceFacts sequence.
    ///
    /// Tree height and page count remain mechanism-owned in `SequenceMeasure`.
    /// This value carries only profile compatibility, exact checkpoint count, and
    /// position-independent source-segment facts.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct SourceFactsSequenceSummary {
        pub(super) checkpoint_spacing_utf16: u64,
        pub(super) content_fingerprint_algorithm: u32,
        pub(super) checkpoint_count: u64,
        pub(super) segment: SourceFactSegmentSummary,
        pub(super) commitment: SourceFactsSequenceCommitment,
    }

    pub(super) struct SourceFactsSequenceSpec;

    impl SequenceSpec for SourceFactsSequenceSpec {
        type Summary = SourceFactsSequenceSummary;
        type Error = SourceFactsAssemblyError;

        fn leaf_summary(
            payload: &[u8],
            inspection: &mut SequenceSpecInspection,
        ) -> Result<Option<Self::Summary>, Self::Error> {
            decode_source_fact_sequence_leaf(payload, inspection)
                .map(|leaf| leaf.map(|leaf| leaf.summary))
        }

        fn branch_measure(
            payload: &[u8],
            _inspection: &mut SequenceSpecInspection,
        ) -> Result<Option<SequenceMeasure<Self::Summary>>, Self::Error> {
            if payload.get(..4) != Some(SOURCE_FACT_SEQUENCE_BRANCH_MAGIC.as_slice()) {
                return Ok(None);
            }
            if payload.len() != SOURCE_FACT_SEQUENCE_BRANCH_BYTES {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "branch payload has the wrong length",
                ));
            }
            let mut cursor = 4;
            let schema = read_u32(payload, &mut cursor)?;
            if schema != SOURCE_FACT_SEQUENCE_SCHEMA {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "branch schema is unsupported",
                ));
            }
            let leaves = read_u64(payload, &mut cursor)?;
            let height = read_u16(payload, &mut cursor)?;
            let checkpoint_spacing_utf16 = read_u64(payload, &mut cursor)?;
            let content_fingerprint_algorithm = read_u32(payload, &mut cursor)?;
            let checkpoint_count = read_u64(payload, &mut cursor)?;
            let segment = decode_segment_summary(payload, &mut cursor)?;
            let commitment = decode_sequence_commitment(payload, &mut cursor)?;
            if cursor != payload.len() {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "branch payload has trailing bytes",
                ));
            }
            validate_sequence_profile(checkpoint_spacing_utf16, content_fingerprint_algorithm)?;
            Ok(Some(SequenceMeasure::new(
                SourceFactsSequenceSummary {
                    checkpoint_spacing_utf16,
                    content_fingerprint_algorithm,
                    checkpoint_count,
                    segment,
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
            validate_sequence_profile(
                summary.checkpoint_spacing_utf16,
                summary.content_fingerprint_algorithm,
            )?;
            let mut cursor = 0;
            write_bytes(output, &mut cursor, &SOURCE_FACT_SEQUENCE_BRANCH_MAGIC)?;
            write_u32(output, &mut cursor, SOURCE_FACT_SEQUENCE_SCHEMA)?;
            write_u64(output, &mut cursor, measure.leaves())?;
            write_u16(output, &mut cursor, measure.height())?;
            write_u64(output, &mut cursor, summary.checkpoint_spacing_utf16)?;
            write_u32(output, &mut cursor, summary.content_fingerprint_algorithm)?;
            write_u64(output, &mut cursor, summary.checkpoint_count)?;
            encode_segment_summary(summary.segment, output, &mut cursor)?;
            encode_sequence_commitment(summary.commitment, output, &mut cursor)?;
            debug_assert_eq!(cursor, SOURCE_FACT_SEQUENCE_BRANCH_BYTES);
            Ok(cursor)
        }

        fn combine(
            left: Self::Summary,
            right: Self::Summary,
        ) -> Result<Self::Summary, Self::Error> {
            if left.checkpoint_spacing_utf16 != right.checkpoint_spacing_utf16
                || left.content_fingerprint_algorithm != right.content_fingerprint_algorithm
            {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "adjacent pages use different scan profiles",
                ));
            }
            Ok(Self::Summary {
                checkpoint_spacing_utf16: left.checkpoint_spacing_utf16,
                content_fingerprint_algorithm: left.content_fingerprint_algorithm,
                checkpoint_count: left
                    .checkpoint_count
                    .checked_add(right.checkpoint_count)
                    .ok_or(SourceFactsAssemblyError::CounterExhausted)?,
                segment: left
                    .segment
                    .checked_followed_by(right.segment)
                    .ok_or(SourceFactsAssemblyError::CanonicalSummaryMismatch)?,
                commitment: combine_sequence_commitments(left.commitment, right.commitment),
            })
        }

        fn invalid(message: &'static str) -> Self::Error {
            SourceFactsAssemblyError::CorruptPersistentSequence(message)
        }
    }

    pub(super) struct DecodedSourceFactSequenceLeaf<'payload> {
        pub(super) summary: SourceFactsSequenceSummary,
        pub(super) content_digest: SourceFactPageDigest,
        checkpoint_bytes: &'payload [u8],
    }

    impl DecodedSourceFactSequenceLeaf<'_> {
        pub(super) fn checkpoint_count(&self) -> usize {
            self.checkpoint_bytes.len() / SOURCE_FACT_SEGMENT_ENCODED_BYTES
        }

        pub(super) fn checkpoint(
            &self,
            ordinal: usize,
        ) -> Result<Option<SourceFactRelativeCheckpoint>, SourceFactsAssemblyError> {
            if ordinal >= self.checkpoint_count() {
                return Ok(None);
            }
            let start = ordinal
                .checked_mul(SOURCE_FACT_SEGMENT_ENCODED_BYTES)
                .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
            let end = start
                .checked_add(SOURCE_FACT_SEGMENT_ENCODED_BYTES)
                .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
            let bytes = self.checkpoint_bytes.get(start..end).ok_or(
                SourceFactsAssemblyError::CorruptPersistentSequence(
                    "relative checkpoint range is truncated",
                ),
            )?;
            let mut cursor = 0;
            let summary = decode_segment_summary(bytes, &mut cursor)?;
            if cursor != bytes.len() {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "relative checkpoint has trailing bytes",
                ));
            }
            Ok(Some(SourceFactRelativeCheckpoint { summary }))
        }
    }

    pub(super) fn encode_source_fact_sequence_leaf(
        page: &SourceFactCanonicalPage,
        profile: SourceFactsScanProfile,
        output: &mut [u8; ARENA_PAGE_BYTES],
    ) -> Result<usize, SourceFactsAssemblyError> {
        validate_scan_profile(profile).map_err(|_| SourceFactsAssemblyError::InvalidProfile)?;
        if page.checkpoints.is_empty()
            || page.checkpoints.len() > SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX
        {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "canonical leaf checkpoint count is invalid",
            ));
        }
        if page.checkpoints.last().map(|checkpoint| checkpoint.summary) != Some(page.summary) {
            return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch);
        }
        validate_relative_checkpoint_summaries(
            profile.checkpoint_spacing_utf16,
            page.summary,
            page.checkpoints.len(),
            page.checkpoints.iter().map(|checkpoint| {
                Ok::<SourceFactSegmentSummary, SourceFactsAssemblyError>(checkpoint.summary)
            }),
        )?;
        if source_fact_page_digest(
            profile.checkpoint_spacing_utf16,
            page.summary,
            &page.checkpoints,
        ) != page.content_digest
        {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "canonical leaf digest is invalid",
            ));
        }

        encode_source_fact_sequence_leaf_payload(page, profile, output)
    }

    fn encode_source_fact_sequence_leaf_payload(
        page: &SourceFactCanonicalPage,
        profile: SourceFactsScanProfile,
        output: &mut [u8; ARENA_PAGE_BYTES],
    ) -> Result<usize, SourceFactsAssemblyError> {
        let checkpoint_count = u16::try_from(page.checkpoints.len())
            .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?;
        let encoded_bytes = SOURCE_FACT_SEQUENCE_LEAF_FIXED_BYTES
            .checked_add(
                page.checkpoints
                    .len()
                    .checked_mul(SOURCE_FACT_SEGMENT_ENCODED_BYTES)
                    .ok_or(SourceFactsAssemblyError::CounterExhausted)?,
            )
            .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        if encoded_bytes > output.len() {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "canonical leaf exceeds one arena page",
            ));
        }

        let mut cursor = 0;
        write_bytes(output, &mut cursor, &SOURCE_FACT_SEQUENCE_LEAF_MAGIC)?;
        write_u32(output, &mut cursor, SOURCE_FACT_SEQUENCE_SCHEMA)?;
        write_u64(output, &mut cursor, profile.checkpoint_spacing_utf16)?;
        write_u32(output, &mut cursor, profile.content_fingerprint_algorithm)?;
        write_u16(output, &mut cursor, checkpoint_count)?;
        write_bytes(output, &mut cursor, &page.content_digest.0)?;
        encode_segment_summary(page.summary, output, &mut cursor)?;
        for checkpoint in page.checkpoints.iter().copied() {
            encode_segment_summary(checkpoint.summary, output, &mut cursor)?;
        }
        debug_assert_eq!(cursor, encoded_bytes);
        Ok(cursor)
    }

    /// Test-only wire encoder for proving that semantic validation rejects a
    /// self-consistent forged digest, rather than merely detecting stale bytes.
    #[cfg(test)]
    pub(super) fn encode_source_fact_sequence_leaf_unchecked_for_test(
        page: &SourceFactCanonicalPage,
        profile: SourceFactsScanProfile,
        output: &mut [u8; ARENA_PAGE_BYTES],
    ) -> Result<usize, SourceFactsAssemblyError> {
        validate_scan_profile(profile).map_err(|_| SourceFactsAssemblyError::InvalidProfile)?;
        if page.checkpoints.is_empty()
            || page.checkpoints.len() > SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX
        {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "canonical leaf checkpoint count is invalid",
            ));
        }
        encode_source_fact_sequence_leaf_payload(page, profile, output)
    }

    pub(super) fn decode_source_fact_sequence_leaf<'payload>(
        payload: &'payload [u8],
        inspection: &mut SequenceSpecInspection,
    ) -> Result<Option<DecodedSourceFactSequenceLeaf<'payload>>, SourceFactsAssemblyError> {
        if payload.get(..4) != Some(SOURCE_FACT_SEQUENCE_LEAF_MAGIC.as_slice()) {
            return Ok(None);
        }
        if payload.len() < SOURCE_FACT_SEQUENCE_LEAF_FIXED_BYTES {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "leaf payload is truncated",
            ));
        }
        let mut cursor = 4;
        if read_u32(payload, &mut cursor)? != SOURCE_FACT_SEQUENCE_SCHEMA {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "leaf schema is unsupported",
            ));
        }
        let checkpoint_spacing_utf16 = read_u64(payload, &mut cursor)?;
        let content_fingerprint_algorithm = read_u32(payload, &mut cursor)?;
        validate_sequence_profile(checkpoint_spacing_utf16, content_fingerprint_algorithm)?;
        let checkpoint_count = usize::from(read_u16(payload, &mut cursor)?);
        if !(1..=SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX).contains(&checkpoint_count) {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "leaf checkpoint count is invalid",
            ));
        }
        let expected_bytes = SOURCE_FACT_SEQUENCE_LEAF_FIXED_BYTES
            .checked_add(
                checkpoint_count
                    .checked_mul(SOURCE_FACT_SEGMENT_ENCODED_BYTES)
                    .ok_or(SourceFactsAssemblyError::CounterExhausted)?,
            )
            .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        if payload.len() != expected_bytes {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "leaf payload length does not match its checkpoint count",
            ));
        }
        let content_digest = SourceFactPageDigest(read_array::<32>(payload, &mut cursor)?);
        let segment = decode_segment_summary(payload, &mut cursor)?;
        let checkpoint_bytes =
            payload
                .get(cursor..)
                .ok_or(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "leaf checkpoint payload is truncated",
                ))?;
        let leaf = DecodedSourceFactSequenceLeaf {
            summary: SourceFactsSequenceSummary {
                checkpoint_spacing_utf16,
                content_fingerprint_algorithm,
                checkpoint_count: u64::try_from(checkpoint_count)
                    .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?,
                segment,
                commitment: leaf_sequence_commitment(payload),
            },
            content_digest,
            checkpoint_bytes,
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"flark.source-facts.root-page.v2\0");
        hasher.update(&checkpoint_spacing_utf16.to_le_bytes());
        append_segment_summary_to_hasher(&mut hasher, segment);
        validate_relative_checkpoint_summaries(
            checkpoint_spacing_utf16,
            segment,
            checkpoint_count,
            (0..checkpoint_count).map(|ordinal| {
                let checkpoint = leaf.checkpoint(ordinal)?.ok_or(
                    SourceFactsAssemblyError::CorruptPersistentSequence(
                        "leaf checkpoint disappeared during validation",
                    ),
                )?;
                inspection
                    .charge_hashed_items(1)
                    .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
                hasher.update(b"flark.source-facts.relative-checkpoint.v1\0");
                append_segment_summary_to_hasher(&mut hasher, checkpoint.summary);
                Ok(checkpoint.summary)
            }),
        )?;
        if SourceFactPageDigest(*hasher.finalize().as_bytes()) != content_digest {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "leaf content digest does not match its checkpoints",
            ));
        }
        Ok(Some(leaf))
    }

    pub(super) fn leaf_sequence_commitment(payload: &[u8]) -> SourceFactsSequenceCommitment {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"flark.source-facts.sequence-leaf-coefficients.v2\0");
        hasher.update(payload);
        let mut coefficients = [0_u8; SOURCE_FACT_SEQUENCE_COMMITMENT_LANES * 8];
        hasher.finalize_xof().fill(&mut coefficients);
        let mut values = [0_u64; SOURCE_FACT_SEQUENCE_COMMITMENT_LANES];
        for (index, value) in values.iter_mut().enumerate() {
            let offset = index * 8;
            *value = u64::from_le_bytes(
                coefficients[offset..offset + 8]
                    .try_into()
                    .expect("BLAKE3 XOF u64 lane"),
            ) % COMMITMENT_MODULUS;
        }
        SourceFactsSequenceCommitment {
            values,
            powers: COMMITMENT_BASES,
        }
    }

    pub(super) fn combine_sequence_commitments(
        left: SourceFactsSequenceCommitment,
        right: SourceFactsSequenceCommitment,
    ) -> SourceFactsSequenceCommitment {
        let mut output = SourceFactsSequenceCommitment::empty();
        for lane in 0..SOURCE_FACT_SEQUENCE_COMMITMENT_LANES {
            output.values[lane] = add_mod(
                mul_mod(left.values[lane], right.powers[lane]),
                right.values[lane],
            );
            output.powers[lane] = mul_mod(left.powers[lane], right.powers[lane]);
        }
        output
    }

    pub(super) fn add_mod(left: u64, right: u64) -> u64 {
        crate::mersenne61::add_mod(left, right)
    }

    pub(super) fn mul_mod(left: u64, right: u64) -> u64 {
        crate::mersenne61::multiply_mod(left, right)
    }

    fn encode_sequence_commitment(
        commitment: SourceFactsSequenceCommitment,
        output: &mut [u8],
        cursor: &mut usize,
    ) -> Result<(), SourceFactsAssemblyError> {
        for value in commitment.values {
            write_u64(output, cursor, value)?;
        }
        for power in commitment.powers {
            write_u64(output, cursor, power)?;
        }
        Ok(())
    }

    fn decode_sequence_commitment(
        payload: &[u8],
        cursor: &mut usize,
    ) -> Result<SourceFactsSequenceCommitment, SourceFactsAssemblyError> {
        let mut commitment = SourceFactsSequenceCommitment::empty();
        for value in &mut commitment.values {
            *value = read_u64(payload, cursor)?;
            if *value >= COMMITMENT_MODULUS {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "sequence commitment value is outside its field",
                ));
            }
        }
        for power in &mut commitment.powers {
            *power = read_u64(payload, cursor)?;
            if *power == 0 || *power >= COMMITMENT_MODULUS {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "sequence commitment power is outside its field",
                ));
            }
        }
        Ok(commitment)
    }

    fn validate_relative_checkpoint_summaries<I>(
        checkpoint_spacing_utf16: u64,
        terminal: SourceFactSegmentSummary,
        checkpoint_count: usize,
        summaries: I,
    ) -> Result<(), SourceFactsAssemblyError>
    where
        I: IntoIterator<Item = Result<SourceFactSegmentSummary, SourceFactsAssemblyError>>,
    {
        let maximum_gap = checkpoint_spacing_utf16
            .checked_add(1)
            .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        let mut previous = SourceFactSegmentSummary::default();
        let mut observed = 0_usize;

        for summary in summaries {
            let summary = summary?;
            if observed >= checkpoint_count {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "leaf has more relative checkpoints than declared",
                ));
            }
            if !summary.is_structurally_valid() {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "relative checkpoint summary is malformed",
                ));
            }
            if summary.starts_with_line_feed != terminal.starts_with_line_feed {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "relative checkpoint start boundary flag changed within a leaf",
                ));
            }
            if observed + 1 != checkpoint_count && summary.ends_with_carriage_return {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "non-terminal relative checkpoint carries the leaf end boundary flag",
                ));
            }
            if summary.byte_len <= previous.byte_len || summary.utf16_len <= previous.utf16_len {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "relative checkpoints are not strictly monotonic",
                ));
            }
            if summary.logical_line_breaks < previous.logical_line_breaks {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "relative checkpoint line count regressed",
                ));
            }

            let byte_gap = summary
                .byte_len
                .checked_sub(previous.byte_len)
                .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
            let utf16_gap = summary
                .utf16_len
                .checked_sub(previous.utf16_len)
                .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
            let line_gap = summary
                .logical_line_breaks
                .checked_sub(previous.logical_line_breaks)
                .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
            if utf16_gap > maximum_gap {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "relative checkpoint gap exceeds the scan profile",
                ));
            }
            let maximum_utf8_bytes = utf16_gap.checked_mul(3).ok_or(
                SourceFactsAssemblyError::CorruptPersistentSequence(
                    "relative checkpoint UTF-8 bound overflowed",
                ),
            )?;
            if byte_gap < utf16_gap || byte_gap > maximum_utf8_bytes {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "relative checkpoint byte and UTF-16 deltas are impossible",
                ));
            }
            if line_gap > utf16_gap {
                return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                    "relative checkpoint line delta is impossible",
                ));
            }

            previous = summary;
            observed += 1;
        }

        if observed != checkpoint_count {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "leaf has fewer relative checkpoints than declared",
            ));
        }
        if previous != terminal {
            return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch);
        }
        Ok(())
    }

    fn validate_sequence_profile(
        checkpoint_spacing_utf16: u64,
        content_fingerprint_algorithm: u32,
    ) -> Result<(), SourceFactsAssemblyError> {
        let maximum_spacing = u64::try_from(SOURCE_FACT_CHECKPOINT_SPACING_MAX_UTF16)
            .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?;
        if !(2..=maximum_spacing).contains(&checkpoint_spacing_utf16)
            || content_fingerprint_algorithm != SOURCE_CONTENT_FINGERPRINT_ALGORITHM
        {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "sequence scan profile is unsupported",
            ));
        }
        Ok(())
    }

    fn encode_segment_summary(
        summary: SourceFactSegmentSummary,
        output: &mut [u8],
        cursor: &mut usize,
    ) -> Result<(), SourceFactsAssemblyError> {
        if !summary.is_structurally_valid() {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "segment summary is malformed",
            ));
        }
        write_u64(output, cursor, summary.byte_len)?;
        write_u64(output, cursor, summary.utf16_len)?;
        write_u64(output, cursor, summary.logical_line_breaks)?;
        for word in summary.rolling_hash.words {
            write_u32(output, cursor, word)?;
        }
        let flags = u8::from(summary.starts_with_line_feed)
            | (u8::from(summary.ends_with_carriage_return) << 1);
        write_bytes(output, cursor, &[flags])
    }

    fn decode_segment_summary(
        payload: &[u8],
        cursor: &mut usize,
    ) -> Result<SourceFactSegmentSummary, SourceFactsAssemblyError> {
        let byte_len = read_u64(payload, cursor)?;
        let utf16_len = read_u64(payload, cursor)?;
        let logical_line_breaks = read_u64(payload, cursor)?;
        let mut words = [0_u32; 4];
        for word in &mut words {
            *word = read_u32(payload, cursor)?;
        }
        let flags = read_array::<1>(payload, cursor)?[0];
        if flags & !0b11 != 0 {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "segment summary has unknown boundary flags",
            ));
        }
        let summary = SourceFactSegmentSummary {
            byte_len,
            utf16_len,
            logical_line_breaks,
            rolling_hash: SourceContentHash128 { words },
            starts_with_line_feed: flags & 1 != 0,
            ends_with_carriage_return: flags & 2 != 0,
        };
        if !summary.is_structurally_valid() {
            return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "segment summary is malformed",
            ));
        }
        Ok(summary)
    }

    fn write_u16(
        output: &mut [u8],
        cursor: &mut usize,
        value: u16,
    ) -> Result<(), SourceFactsAssemblyError> {
        write_bytes(output, cursor, &value.to_le_bytes())
    }

    fn write_u32(
        output: &mut [u8],
        cursor: &mut usize,
        value: u32,
    ) -> Result<(), SourceFactsAssemblyError> {
        write_bytes(output, cursor, &value.to_le_bytes())
    }

    fn write_u64(
        output: &mut [u8],
        cursor: &mut usize,
        value: u64,
    ) -> Result<(), SourceFactsAssemblyError> {
        write_bytes(output, cursor, &value.to_le_bytes())
    }

    fn write_bytes(
        output: &mut [u8],
        cursor: &mut usize,
        bytes: &[u8],
    ) -> Result<(), SourceFactsAssemblyError> {
        let end = cursor
            .checked_add(bytes.len())
            .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        let target = output.get_mut(*cursor..end).ok_or(
            SourceFactsAssemblyError::CorruptPersistentSequence(
                "persistent sequence encoding exceeds its fixed buffer",
            ),
        )?;
        target.copy_from_slice(bytes);
        *cursor = end;
        Ok(())
    }

    fn read_u16(payload: &[u8], cursor: &mut usize) -> Result<u16, SourceFactsAssemblyError> {
        Ok(u16::from_le_bytes(read_array(payload, cursor)?))
    }

    fn read_u32(payload: &[u8], cursor: &mut usize) -> Result<u32, SourceFactsAssemblyError> {
        Ok(u32::from_le_bytes(read_array(payload, cursor)?))
    }

    fn read_u64(payload: &[u8], cursor: &mut usize) -> Result<u64, SourceFactsAssemblyError> {
        Ok(u64::from_le_bytes(read_array(payload, cursor)?))
    }

    fn read_array<const N: usize>(
        payload: &[u8],
        cursor: &mut usize,
    ) -> Result<[u8; N], SourceFactsAssemblyError> {
        let end = cursor
            .checked_add(N)
            .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        let bytes = payload.get(*cursor..end).ok_or(
            SourceFactsAssemblyError::CorruptPersistentSequence(
                "persistent sequence payload is truncated",
            ),
        )?;
        let mut output = [0_u8; N];
        output.copy_from_slice(bytes);
        *cursor = end;
        Ok(output)
    }
}

/// One derived M1.1-compatible absolute page. Canonical storage remains
/// relative; this fixed buffer is materialized only at the publication edge.
pub(crate) struct SourceFactsAbsolutePage {
    content_digest: SourceFactPageDigest,
    checkpoints: [SourceFactCheckpoint; SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX],
    checkpoint_count: usize,
}

impl SourceFactsAbsolutePage {
    pub(crate) const fn content_digest(&self) -> SourceFactPageDigest {
        self.content_digest
    }

    pub(crate) fn checkpoints(&self) -> &[SourceFactCheckpoint] {
        &self.checkpoints[..self.checkpoint_count]
    }
}

/// Locates one canonical leaf in logarithmic tree work and derives its
/// absolute checkpoints into fixed caller-owned storage. No absolute suffix
/// coordinates or page objects remain resident in the persistent root.
fn materialize_source_facts_absolute_page(
    sequence: crate::measured_sequence::MeasuredSequenceRef<
        '_,
        persistent_sequence_codec::SourceFactsSequenceSpec,
    >,
    arena: &crate::storage::PageArena,
    ordinal: u64,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<Option<SourceFactsAbsolutePage>, SourceFactsAssemblyError> {
    let Some(located) = sequence.locate_leaf_with_prefix(arena, ordinal, inspection)? else {
        return Ok(None);
    };
    let payload = arena.payload(located.id)?;
    inspection
        .spec
        .charge_payload_bytes(payload.len())
        .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
    let leaf =
        persistent_sequence_codec::decode_source_fact_sequence_leaf(payload, &mut inspection.spec)?
            .ok_or(SourceFactsAssemblyError::CorruptPersistentSequence(
                "located sequence leaf has the wrong payload kind",
            ))?;
    if leaf.summary != located.summary {
        return Err(SourceFactsAssemblyError::CorruptPersistentSequence(
            "located leaf summary does not match its routing measure",
        ));
    }
    let prefix = located
        .prefix
        .map_or(SourceFactSegmentSummary::default(), |summary| {
            summary.segment
        });
    let mut output = SourceFactsAbsolutePage {
        content_digest: leaf.content_digest,
        checkpoints: [SourceFactCheckpoint::default(); SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX],
        checkpoint_count: leaf.checkpoint_count(),
    };
    for ordinal in 0..leaf.checkpoint_count() {
        let relative = leaf.checkpoint(ordinal)?.ok_or(
            SourceFactsAssemblyError::CorruptPersistentSequence(
                "canonical leaf checkpoint is missing",
            ),
        )?;
        let absolute = prefix
            .checked_followed_by(relative.summary)
            .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        output.checkpoints[ordinal] = SourceFactCheckpoint {
            byte_offset: absolute.byte_len,
            utf16_offset: absolute.utf16_len,
            logical_line_breaks: absolute.logical_line_breaks,
            rolling_hash: absolute.rolling_hash,
        };
    }
    Ok(Some(output))
}

/// Fresh root binding around reusable page-local facts.
///
/// `absolute_checkpoints` is a derived M1.1 transport projection. It is
/// deliberately excluded from canonical identity and content digests; the
/// incremental path can therefore bind the same canonical page at a different
/// prefix without treating rebased coordinates as new source-fact authority.
#[derive(Debug)]
pub struct SourceFactRootPage {
    canonical: Arc<SourceFactCanonicalPage>,
    absolute_checkpoints: Box<[SourceFactCheckpoint]>,
}

impl SourceFactRootPage {
    #[must_use]
    pub fn canonical(&self) -> &Arc<SourceFactCanonicalPage> {
        &self.canonical
    }

    #[must_use]
    pub fn content_digest(&self) -> SourceFactPageDigest {
        self.canonical.content_digest()
    }

    #[must_use]
    pub fn checkpoints(&self) -> &[SourceFactCheckpoint] {
        &self.absolute_checkpoints
    }
}

/// Complete, validated source facts for one exact source authority.
///
/// The checkpoint digest authenticates the ordered fact stream. It does not
/// replace `source`, whose immutable root and revision remain the authority.
#[derive(Debug)]
pub struct SourceFactsRoot {
    source: SourceVersion,
    profile: SourceFactsScanProfile,
    scan_id: SourceFactsScanId,
    sequence_digest: SourceFactSequenceDigest,
    summary: SourceFactSegmentSummary,
    page_count: u64,
    checkpoint_count: u64,
    pages: LinkedList<Arc<SourceFactRootPage>>,
}

impl SourceFactsRoot {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn profile(&self) -> SourceFactsScanProfile {
        self.profile
    }

    #[must_use]
    pub const fn scan_id(&self) -> SourceFactsScanId {
        self.scan_id
    }

    #[must_use]
    pub const fn sequence_digest(&self) -> SourceFactSequenceDigest {
        self.sequence_digest
    }

    #[must_use]
    pub const fn fingerprint(&self) -> SourceContentFingerprint {
        SourceContentFingerprint {
            algorithm: self.profile.content_fingerprint_algorithm,
            byte_len: self.summary.byte_len,
            utf16_len: self.summary.utf16_len,
            rolling_hash: self.summary.rolling_hash,
        }
    }

    #[must_use]
    pub const fn logical_line_breaks(&self) -> u64 {
        self.summary.logical_line_breaks
    }

    /// Returns the exact composable summary at this root.
    #[must_use]
    pub const fn summary(&self) -> SourceFactSegmentSummary {
        self.summary
    }

    #[must_use]
    pub const fn page_count(&self) -> u64 {
        self.page_count
    }

    #[must_use]
    pub const fn checkpoint_count(&self) -> u64 {
        self.checkpoint_count
    }

    #[must_use]
    pub fn pages(
        &self,
    ) -> impl DoubleEndedIterator<Item = &Arc<SourceFactRootPage>> + ExactSizeIterator {
        self.pages.iter()
    }
}

/// Move-only capability proving complete facts for one still-live exact lease.
pub struct CertifiedSource {
    lease: SourceSnapshotLease,
    parser_profile: ParserProfileId,
    facts: SourceFactsRoot,
    coverage: SourceFactsCoverage,
}

impl fmt::Debug for CertifiedSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CertifiedSource")
            .field("source", &self.lease.version())
            .field("parser_profile", &self.parser_profile)
            .field("facts", &self.facts)
            .field("coverage", &self.coverage)
            .finish()
    }
}

impl CertifiedSource {
    /// Returns the exact authority certified by these facts.
    #[must_use]
    pub fn source(&self) -> SourceVersion {
        self.lease.version()
    }

    /// Returns the exact parser grammar/syntax profile authorized by the gate.
    #[must_use]
    pub const fn parser_profile(&self) -> ParserProfileId {
        self.parser_profile
    }

    #[must_use]
    pub const fn facts(&self) -> &SourceFactsRoot {
        &self.facts
    }

    #[must_use]
    pub const fn coverage(&self) -> SourceFactsCoverage {
        self.coverage
    }

    /// Mints the one additional immutable lease consumed by the exact parser.
    ///
    /// The certification itself remains move-only and must later be consumed
    /// by candidate derivation. This narrow duplication lets parsing and the
    /// final authority join inspect the same persistent source root without
    /// materializing or flattening it.
    #[must_use]
    pub fn exact_parse_lease(&self) -> SourceSnapshotLease {
        self.lease.duplicate()
    }

    /// Transfers the exact lease and fact root to their next owner.
    #[must_use]
    pub fn into_parts(self) -> (SourceSnapshotLease, ParserProfileId, SourceFactsRoot) {
        (self.lease, self.parser_profile, self.facts)
    }
}

type SourceFactsMeasuredRoot =
    CommittedMeasuredSequenceRoot<persistent_sequence_codec::SourceFactsSequenceSpec>;
type SourceFactsMeasuredBuildRoot =
    MeasuredSequenceBuildRoot<persistent_sequence_codec::SourceFactsSequenceSpec>;
type SourceFactsMeasuredBuilder =
    ResumableMeasuredSequenceBuilder<persistent_sequence_codec::SourceFactsSequenceSpec>;
type SourceFactsMeasuredSeal =
    MeasuredSequenceSeal<persistent_sequence_codec::SourceFactsSequenceSpec>;
/// Cumulative bounded work used to promote one clean certification into the
/// actor-owned persistent SourceFacts index.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PersistentSourceFactsWork {
    node_headers_decoded: u64,
    payload_bytes_inspected: u64,
    checkpoints_hashed: u64,
    summary_combinations: u64,
    leaves_adopted: usize,
    branches_allocated: usize,
    branch_payload_bytes: usize,
    nodes_visited: usize,
    leaves_reused: usize,
    committed_leaves_retained: usize,
    leaves_deleted: usize,
    maximum_atomic_height: u16,
    seal_transitions: usize,
}

impl PersistentSourceFactsWork {
    pub(crate) fn from_inspection(inspection: SequenceInspectionReceipt) -> Self {
        Self {
            node_headers_decoded: inspection.node_headers_decoded,
            payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
            checkpoints_hashed: inspection.spec.spec_items_hashed,
            summary_combinations: inspection.summary_combinations,
            ..Self::default()
        }
    }

    fn from_receipt(receipt: SequenceMutationReceipt, seal_transitions: usize) -> Self {
        Self {
            node_headers_decoded: receipt.inspection.node_headers_decoded,
            payload_bytes_inspected: receipt.inspection.spec.payload_bytes_inspected,
            checkpoints_hashed: receipt.inspection.spec.spec_items_hashed,
            summary_combinations: receipt.inspection.summary_combinations,
            leaves_adopted: receipt.leaves_adopted,
            branches_allocated: receipt.branches_allocated,
            branch_payload_bytes: receipt.branch_payload_bytes,
            nodes_visited: receipt.nodes_visited,
            leaves_reused: receipt.leaves_reused,
            committed_leaves_retained: receipt.committed_leaves_retained,
            leaves_deleted: receipt.leaves_deleted,
            maximum_atomic_height: receipt.maximum_atomic_height,
            seal_transitions,
        }
    }

    pub(crate) fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            node_headers_decoded: self
                .node_headers_decoded
                .checked_add(other.node_headers_decoded)?,
            payload_bytes_inspected: self
                .payload_bytes_inspected
                .checked_add(other.payload_bytes_inspected)?,
            checkpoints_hashed: self
                .checkpoints_hashed
                .checked_add(other.checkpoints_hashed)?,
            summary_combinations: self
                .summary_combinations
                .checked_add(other.summary_combinations)?,
            leaves_adopted: self.leaves_adopted.checked_add(other.leaves_adopted)?,
            branches_allocated: self
                .branches_allocated
                .checked_add(other.branches_allocated)?,
            branch_payload_bytes: self
                .branch_payload_bytes
                .checked_add(other.branch_payload_bytes)?,
            nodes_visited: self.nodes_visited.checked_add(other.nodes_visited)?,
            leaves_reused: self.leaves_reused.checked_add(other.leaves_reused)?,
            committed_leaves_retained: self
                .committed_leaves_retained
                .checked_add(other.committed_leaves_retained)?,
            leaves_deleted: self.leaves_deleted.checked_add(other.leaves_deleted)?,
            maximum_atomic_height: self.maximum_atomic_height.max(other.maximum_atomic_height),
            seal_transitions: self.seal_transitions.checked_add(other.seal_transitions)?,
        })
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
    pub const fn checkpoints_hashed(self) -> u64 {
        self.checkpoints_hashed
    }

    #[must_use]
    pub const fn summary_combinations(self) -> u64 {
        self.summary_combinations
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
    pub const fn nodes_visited(self) -> usize {
        self.nodes_visited
    }

    #[must_use]
    pub const fn leaves_reused(self) -> usize {
        self.leaves_reused
    }

    #[must_use]
    pub const fn committed_leaves_retained(self) -> usize {
        self.committed_leaves_retained
    }

    #[must_use]
    pub const fn leaves_deleted(self) -> usize {
        self.leaves_deleted
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

/// Actor-owned persistent SourceFacts index for one exact certified source.
///
/// The immutable measured sequence is the storage and publication authority.
/// `CertifiedSource` temporarily retains the linked-list scan projection only
/// for exact parser handoff and clean-oracle evidence; both forms originate
/// from the same canonical page content, and only this root survives as the
/// runtime's reusable index.
pub(crate) struct PersistentSourceFactsRoot {
    source: SourceVersion,
    parser_profile: ParserProfileId,
    coverage: SourceFactsCoverage,
    profile: SourceFactsScanProfile,
    summary: SourceFactSegmentSummary,
    page_count: u64,
    checkpoint_count: u64,
    expected_commitment: persistent_sequence_codec::SourceFactsSequenceCommitment,
    work: PersistentSourceFactsWork,
    tree: Option<SourceFactsMeasuredRoot>,
}

/// Opaque equality witness for the exact persistent SourceFacts authority.
///
/// Consumers may copy and compare this value, but cannot recover its arena
/// identity or commitment components. This lets incremental adoption prove
/// that an exact base remained installed without making raw node IDs an API.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct PersistentSourceFactsRootAuthoritySnapshot {
    tree_root: Option<ArenaId>,
    ordered_commitment: persistent_sequence_codec::SourceFactsSequenceCommitment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentSourceFactsPageLocation {
    pub(crate) id: ArenaId,
    pub(crate) ordinal: u64,
    pub(crate) byte_start: u64,
    pub(crate) byte_end: u64,
}

impl fmt::Debug for PersistentSourceFactsRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentSourceFactsRoot")
            .field("source", &self.source)
            .field("parser_profile", &self.parser_profile)
            .field("coverage", &self.coverage)
            .field("profile", &self.profile)
            .field("summary", &self.summary)
            .field("page_count", &self.page_count)
            .field("checkpoint_count", &self.checkpoint_count)
            .field("expected_commitment", &self.expected_commitment)
            .field("work", &self.work)
            .field("tree", &self.tree)
            .finish()
    }
}

impl PersistentSourceFactsRoot {
    pub(crate) const fn source(&self) -> SourceVersion {
        self.source
    }

    pub(crate) const fn parser_profile(&self) -> ParserProfileId {
        self.parser_profile
    }

    pub(crate) const fn coverage(&self) -> SourceFactsCoverage {
        self.coverage
    }

    pub(crate) const fn profile(&self) -> SourceFactsScanProfile {
        self.profile
    }

    pub(crate) const fn summary(&self) -> SourceFactSegmentSummary {
        self.summary
    }

    pub(crate) const fn page_count(&self) -> u64 {
        self.page_count
    }

    pub(crate) const fn checkpoint_count(&self) -> u64 {
        self.checkpoint_count
    }

    pub(crate) const fn work(&self) -> PersistentSourceFactsWork {
        self.work
    }

    pub(crate) fn checkpoint_root_guard128(&self) -> [u32; 4] {
        if self.page_count == 0
            && self.checkpoint_count == 0
            && self.summary == SourceFactSegmentSummary::default()
        {
            return [0; 4];
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"flark.source-facts.persistent-checkpoint-root-guard.v2\0");
        hasher.update(&PERSISTENT_SOURCE_FACTS_CHECKPOINT_ROOT_GUARD_ALGORITHM.to_le_bytes());
        hasher.update(&self.parser_profile.get().to_le_bytes());
        hasher.update(&self.profile.checkpoint_spacing_utf16.to_le_bytes());
        hasher.update(&self.profile.content_fingerprint_algorithm.to_le_bytes());
        hasher.update(&self.page_count.to_le_bytes());
        hasher.update(&self.checkpoint_count.to_le_bytes());
        hasher.update(&self.summary.byte_len.to_le_bytes());
        hasher.update(&self.summary.utf16_len.to_le_bytes());
        hasher.update(&self.summary.logical_line_breaks.to_le_bytes());
        for word in self.summary.rolling_hash.words {
            hasher.update(&word.to_le_bytes());
        }
        hasher.update(&[u8::from(self.summary.starts_with_line_feed)
            | (u8::from(self.summary.ends_with_carriage_return) << 1)]);
        for value in self.expected_commitment.values {
            hasher.update(&value.to_le_bytes());
        }
        for power in self.expected_commitment.powers {
            hasher.update(&power.to_le_bytes());
        }
        let digest = hasher.finalize();
        let bytes = digest.as_bytes();
        [
            u32::from_le_bytes(bytes[0..4].try_into().expect("BLAKE3 lane")),
            u32::from_le_bytes(bytes[4..8].try_into().expect("BLAKE3 lane")),
            u32::from_le_bytes(bytes[8..12].try_into().expect("BLAKE3 lane")),
            u32::from_le_bytes(bytes[12..16].try_into().expect("BLAKE3 lane")),
        ]
    }

    pub(crate) fn authority_snapshot(&self) -> PersistentSourceFactsRootAuthoritySnapshot {
        PersistentSourceFactsRootAuthoritySnapshot {
            tree_root: self.tree.as_ref().and_then(|tree| tree.as_ref().root_id()),
            ordered_commitment: self.expected_commitment,
        }
    }
    pub(crate) fn locate_byte(
        &self,
        arena: &PageArena,
        byte_position: u64,
        inspection: &mut SequenceInspectionReceipt,
    ) -> Result<Option<PersistentSourceFactsPageLocation>, SourceFactsAssemblyError> {
        let Some(tree) = self.tree.as_ref() else {
            return Ok(None);
        };
        let Some(located) = tree.as_ref().locate_leaf_containing_metric(
            arena,
            byte_position,
            |summary| summary.segment.byte_len,
            inspection,
        )?
        else {
            return Ok(None);
        };
        let byte_start = located.prefix.map_or(0, |summary| summary.segment.byte_len);
        let byte_end = byte_start
            .checked_add(located.summary.segment.byte_len)
            .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        if byte_end > self.summary.byte_len {
            return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch);
        }
        Ok(Some(PersistentSourceFactsPageLocation {
            id: located.id,
            ordinal: located.ordinal,
            byte_start,
            byte_end,
        }))
    }

    pub(crate) fn page_id(
        &self,
        arena: &PageArena,
        ordinal: u64,
        inspection: &mut SequenceInspectionReceipt,
    ) -> Result<Option<ArenaId>, SourceFactsAssemblyError> {
        let Some(tree) = self.tree.as_ref() else {
            return Ok(None);
        };
        Ok(tree
            .as_ref()
            .locate_leaf_with_prefix(arena, ordinal, inspection)?
            .map(|located| located.id))
    }

    pub(crate) fn materialize_page(
        &self,
        arena: &PageArena,
        ordinal: u64,
        inspection: &mut SequenceInspectionReceipt,
    ) -> Result<Option<SourceFactsAbsolutePage>, SourceFactsAssemblyError> {
        let Some(tree) = self.tree.as_ref() else {
            return Ok(None);
        };
        materialize_source_facts_absolute_page(tree.as_ref(), arena, ordinal, inspection)
    }

    pub(crate) fn release(
        self,
        arena: &mut PageArena,
    ) -> Result<(), PersistentSourceFactsRootReleaseFailure> {
        let Self {
            source,
            parser_profile,
            coverage,
            profile,
            summary,
            page_count,
            checkpoint_count,
            expected_commitment,
            work,
            tree,
        } = self;
        let Some(tree) = tree else {
            return Ok(());
        };
        match tree.release(arena) {
            Ok(()) => Ok(()),
            Err(CommittedMeasuredSequenceRootReleaseFailure { error, root }) => {
                Err(PersistentSourceFactsRootReleaseFailure {
                    error,
                    root: Box::new(Self {
                        source,
                        parser_profile,
                        coverage,
                        profile,
                        summary,
                        page_count,
                        checkpoint_count,
                        expected_commitment,
                        work,
                        tree: Some(root),
                    }),
                })
            }
        }
    }
}

pub(crate) struct PersistentSourceFactsRootReleaseFailure {
    pub(crate) error: ArenaError,
    pub(crate) root: Box<PersistentSourceFactsRoot>,
}

fn persistent_source_facts_commitment_for_range(
    arena: &PageArena,
    tree: &SourceFactsMeasuredRoot,
    range: std::ops::Range<u64>,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<persistent_sequence_codec::SourceFactsSequenceCommitment, SourceFactsAssemblyError> {
    Ok(tree
        .as_ref()
        .range_summary(arena, range, inspection)?
        .map_or_else(
            persistent_sequence_codec::SourceFactsSequenceCommitment::empty,
            |summary| summary.commitment,
        ))
}

/// One atomic persistent splice plus work attributable only to that splice.
///
/// The returned root keeps its existing cumulative accounting contract:
/// replacement promotion work plus `splice_work`.
pub(crate) struct PersistentSourceFactsSpliceOutput {
    root: PersistentSourceFactsRoot,
    splice_work: PersistentSourceFactsWork,
}

#[cfg(test)]
pub(crate) fn splice_persistent_source_facts_atomic(
    arena: &mut PageArena,
    base: &PersistentSourceFactsRoot,
    replacement: &PersistentSourceFactsRoot,
    page_range: std::ops::Range<u64>,
    target_source: SourceVersion,
) -> Result<PersistentSourceFactsRoot, SourceFactsAssemblyError> {
    splice_persistent_source_facts_atomic_with_receipt(
        arena,
        base,
        replacement,
        page_range,
        target_source,
    )
    .map(PersistentSourceFactsSpliceOutput::into_parts)
    .map(|(root, _splice_work)| root)
}

impl PersistentSourceFactsSpliceOutput {
    pub(crate) fn into_parts(self) -> (PersistentSourceFactsRoot, PersistentSourceFactsWork) {
        (self.root, self.splice_work)
    }
}

pub(crate) fn splice_persistent_source_facts_atomic_with_receipt(
    arena: &mut PageArena,
    base: &PersistentSourceFactsRoot,
    replacement: &PersistentSourceFactsRoot,
    page_range: std::ops::Range<u64>,
    target_source: SourceVersion,
) -> Result<PersistentSourceFactsSpliceOutput, SourceFactsAssemblyError> {
    if base.coverage != SourceFactsCoverage::CleanEof
        || !matches!(replacement.coverage, SourceFactsCoverage::ExactRange { .. })
        || base.profile != replacement.profile
        || base.parser_profile != replacement.parser_profile
        || replacement.source != target_source
        || page_range.start > page_range.end
        || page_range.end > base.page_count
    {
        return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch);
    }
    let base_tree = base
        .tree
        .as_ref()
        .ok_or(SourceFactsAssemblyError::CanonicalSummaryMismatch)?;
    let mut receipt = SequenceMutationReceipt::default();
    let prefix_commitment = persistent_source_facts_commitment_for_range(
        arena,
        base_tree,
        0..page_range.start,
        &mut receipt.inspection,
    )?;
    let suffix_commitment = persistent_source_facts_commitment_for_range(
        arena,
        base_tree,
        page_range.end..base.page_count,
        &mut receipt.inspection,
    )?;
    let expected_commitment = persistent_sequence_codec::combine_sequence_commitments(
        persistent_sequence_codec::combine_sequence_commitments(
            prefix_commitment,
            replacement.expected_commitment,
        ),
        suffix_commitment,
    );
    let mut session = arena.begin_build()?;
    let replacement_root = replacement
        .tree
        .as_ref()
        .map(|tree| retain_committed_measured_sequence_root(&mut session, tree, &mut receipt))
        .transpose()?;
    let root = splice_measured_sequence_atomic::<persistent_sequence_codec::SourceFactsSequenceSpec>(
        &mut session,
        base_tree,
        page_range,
        replacement_root,
        &mut receipt,
    )?;
    let Some(root) = root else {
        drop(session);
        if target_source.byte_len() != 0 || target_source.utf16_len() != 0 {
            return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch);
        }
        if expected_commitment != persistent_sequence_codec::SourceFactsSequenceCommitment::empty()
        {
            return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch);
        }
        let splice_work = PersistentSourceFactsWork::from_receipt(receipt, 0);
        let work = replacement
            .work
            .checked_add(splice_work)
            .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        return Ok(PersistentSourceFactsSpliceOutput {
            root: PersistentSourceFactsRoot {
                source: target_source,
                parser_profile: base.parser_profile,
                coverage: SourceFactsCoverage::CleanEof,
                profile: base.profile,
                summary: SourceFactSegmentSummary::default(),
                page_count: 0,
                checkpoint_count: 0,
                expected_commitment,
                work,
                tree: None,
            },
            splice_work,
        });
    };
    let build = session.suspend()?;
    let mut seal = match begin_measured_sequence_seal(arena, build, root) {
        Ok(seal) => seal,
        Err(failure) => {
            let error = failure.error;
            let root = failure.root;
            if let Err(abort_error) = arena.abort_build(failure.build) {
                let _root = root;
                panic!(
                    "SourceFacts build created by an arena was rejected by the same arena: \
                     {abort_error}"
                );
            }
            return Err(error.into());
        }
    };
    let mut seal_transitions = 0_usize;
    let tree = loop {
        let poll = match seal.poll(arena, 1) {
            Ok(poll) => poll,
            Err(error) => {
                abort_source_facts_seal_after_failure(arena, seal);
                return Err(error.into());
            }
        };
        let Some(next_seal_transitions) = seal_transitions.checked_add(poll.transitions) else {
            abort_source_facts_seal_after_failure(arena, seal);
            return Err(SourceFactsAssemblyError::CounterExhausted);
        };
        seal_transitions = next_seal_transitions;
        if let Some(tree) = poll.root {
            break tree;
        }
    };
    let measure = match tree.as_ref().summary(arena, &mut receipt.inspection) {
        Ok(Some(measure)) => measure,
        Ok(None) => {
            release_measured_tree_after_failure(arena, tree);
            return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch);
        }
        Err(error) => {
            release_measured_tree_after_failure(arena, tree);
            return Err(error);
        }
    };
    let summary = measure.summary();
    let target_byte_len = match u64::try_from(target_source.byte_len()) {
        Ok(byte_len) => byte_len,
        Err(_) => {
            release_measured_tree_after_failure(arena, tree);
            return Err(SourceFactsAssemblyError::CounterExhausted);
        }
    };
    let target_utf16_len = match u64::try_from(target_source.utf16_len()) {
        Ok(utf16_len) => utf16_len,
        Err(_) => {
            release_measured_tree_after_failure(arena, tree);
            return Err(SourceFactsAssemblyError::CounterExhausted);
        }
    };
    if summary.checkpoint_spacing_utf16 != base.profile.checkpoint_spacing_utf16
        || summary.content_fingerprint_algorithm != base.profile.content_fingerprint_algorithm
        || summary.segment.byte_len != target_byte_len
        || summary.segment.utf16_len != target_utf16_len
        || summary.commitment != expected_commitment
    {
        release_measured_tree_after_failure(arena, tree);
        return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch);
    }
    let splice_work = PersistentSourceFactsWork::from_receipt(receipt, seal_transitions);
    let Some(work) = replacement.work.checked_add(splice_work) else {
        release_measured_tree_after_failure(arena, tree);
        return Err(SourceFactsAssemblyError::CounterExhausted);
    };
    Ok(PersistentSourceFactsSpliceOutput {
        root: PersistentSourceFactsRoot {
            source: target_source,
            parser_profile: base.parser_profile,
            coverage: SourceFactsCoverage::CleanEof,
            profile: base.profile,
            summary: summary.segment,
            page_count: measure.leaves(),
            checkpoint_count: summary.checkpoint_count,
            expected_commitment,
            work,
            tree: Some(tree),
        },
        splice_work,
    })
}

fn abort_source_facts_seal_after_failure(arena: &mut PageArena, seal: SourceFactsMeasuredSeal) {
    if let Err(failure) = seal.abort(arena) {
        let error = failure.error;
        let _seal = failure.seal;
        panic!("SourceFacts seal rejected its arena during failure cleanup: {error}");
    }
}

fn release_measured_tree_after_failure(arena: &mut PageArena, tree: SourceFactsMeasuredRoot) {
    if let Err(failure) = tree.release(arena) {
        panic!(
            "SourceFacts tree created by an arena was rejected by the same arena: {}",
            failure.error
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PersistentSourceFactsBuildPhase {
    Start,
    Pushing,
    ReadyForLeaf,
    ReadyForFinish,
    Finishing,
    ReadyForRoot,
    ReadyForSeal,
    Sealing,
    Complete,
    Failed,
}

pub(crate) enum PersistentSourceFactsBuildPoll {
    Pending,
    Complete(Box<PersistentSourceFactsBuildOutput>),
}

pub(crate) struct PersistentSourceFactsBuildOutput {
    pub(crate) certified: CertifiedSource,
    pub(crate) root: PersistentSourceFactsRoot,
}

/// Resumably promotes one clean certification into the actor-owned measured
/// sequence without changing the legacy publication projection.
///
/// A poll allocates at most one leaf or one branch, or performs one seal
/// transition. The exact structural and payload work remains available in the
/// cumulative mutation receipt.
pub(crate) struct PersistentSourceFactsBuild {
    certified: Option<CertifiedSource>,
    pending_pages: LinkedList<Arc<SourceFactRootPage>>,
    completed_pages: LinkedList<Arc<SourceFactRootPage>>,
    builder: Option<SourceFactsMeasuredBuilder>,
    build: Option<CandidateBuild>,
    build_root: Option<SourceFactsMeasuredBuildRoot>,
    seal: Option<SourceFactsMeasuredSeal>,
    phase: PersistentSourceFactsBuildPhase,
    receipt: SequenceMutationReceipt,
    expected_commitment: persistent_sequence_codec::SourceFactsSequenceCommitment,
    seal_transitions: usize,
}

impl PersistentSourceFactsBuild {
    pub(crate) fn new(mut certified: CertifiedSource) -> Self {
        let pending_pages = std::mem::take(&mut certified.facts.pages);
        Self {
            certified: Some(certified),
            pending_pages,
            completed_pages: LinkedList::new(),
            builder: None,
            build: None,
            build_root: None,
            seal: None,
            phase: PersistentSourceFactsBuildPhase::Start,
            receipt: SequenceMutationReceipt::default(),
            expected_commitment: persistent_sequence_codec::SourceFactsSequenceCommitment::empty(),
            seal_transitions: 0,
        }
    }

    fn work(&self) -> PersistentSourceFactsWork {
        PersistentSourceFactsWork::from_receipt(self.receipt, self.seal_transitions)
    }

    pub(crate) fn poll(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<PersistentSourceFactsBuildPoll, SourceFactsAssemblyError> {
        match self.phase {
            PersistentSourceFactsBuildPhase::Start => self.start(arena),
            PersistentSourceFactsBuildPhase::Pushing => self.poll_push(arena),
            PersistentSourceFactsBuildPhase::ReadyForLeaf => self.begin_next_leaf(arena),
            PersistentSourceFactsBuildPhase::ReadyForFinish => self.begin_finish(arena),
            PersistentSourceFactsBuildPhase::Finishing => self.poll_finish(arena),
            PersistentSourceFactsBuildPhase::ReadyForRoot => self.take_root(arena),
            PersistentSourceFactsBuildPhase::ReadyForSeal => self.begin_seal(arena),
            PersistentSourceFactsBuildPhase::Sealing => self.poll_seal(arena),
            PersistentSourceFactsBuildPhase::Complete => {
                Err(SourceFactsAssemblyError::BuilderPoisoned)
            }
            PersistentSourceFactsBuildPhase::Failed => {
                Err(SourceFactsAssemblyError::BuilderPoisoned)
            }
        }
    }

    fn start(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<PersistentSourceFactsBuildPoll, SourceFactsAssemblyError> {
        if self.pending_pages.is_empty() {
            return self.complete_empty();
        }
        let mut payload = [0_u8; ARENA_PAGE_BYTES];
        let payload_len = self.encode_front_page(&mut payload)?;
        let leaf_commitment =
            persistent_sequence_codec::leaf_sequence_commitment(&payload[..payload_len]);
        let result = (|| {
            let mut session = arena.begin_build()?;
            let mut builder = SourceFactsMeasuredBuilder::try_new(&mut session, &mut self.receipt)?;
            let leaf = session.allocate(&payload[..payload_len], &[])?;
            builder.begin_push(&session, leaf, &mut self.receipt)?;
            let build = session.suspend()?;
            Ok::<_, SourceFactsAssemblyError>((builder, build))
        })();
        match result {
            Ok((builder, build)) => {
                self.finish_front_page(leaf_commitment);
                self.builder = Some(builder);
                self.build = Some(build);
                self.phase = PersistentSourceFactsBuildPhase::Pushing;
                Ok(PersistentSourceFactsBuildPoll::Pending)
            }
            Err(error) => {
                self.phase = PersistentSourceFactsBuildPhase::Failed;
                Err(error)
            }
        }
    }

    fn poll_push(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<PersistentSourceFactsBuildPoll, SourceFactsAssemblyError> {
        let progress = self.with_resumed_build(arena, |builder, session, receipt| {
            builder.poll_push(session, receipt)
        })?;
        self.phase = match progress {
            ResumableSequenceProgress::Pending => PersistentSourceFactsBuildPhase::Pushing,
            ResumableSequenceProgress::Complete if self.pending_pages.is_empty() => {
                PersistentSourceFactsBuildPhase::ReadyForFinish
            }
            ResumableSequenceProgress::Complete => PersistentSourceFactsBuildPhase::ReadyForLeaf,
        };
        Ok(PersistentSourceFactsBuildPoll::Pending)
    }

    fn begin_next_leaf(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<PersistentSourceFactsBuildPoll, SourceFactsAssemblyError> {
        let mut payload = [0_u8; ARENA_PAGE_BYTES];
        let payload_len = self.encode_front_page(&mut payload)?;
        let leaf_commitment =
            persistent_sequence_codec::leaf_sequence_commitment(&payload[..payload_len]);
        self.with_resumed_build(arena, |builder, session, receipt| {
            let leaf = session.allocate(&payload[..payload_len], &[])?;
            builder.begin_push(session, leaf, receipt)
        })?;
        self.finish_front_page(leaf_commitment);
        self.phase = PersistentSourceFactsBuildPhase::Pushing;
        Ok(PersistentSourceFactsBuildPoll::Pending)
    }

    fn begin_finish(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<PersistentSourceFactsBuildPoll, SourceFactsAssemblyError> {
        self.with_resumed_build(arena, |builder, session, receipt| {
            builder.begin_finish(session, receipt)
        })?;
        self.phase = PersistentSourceFactsBuildPhase::Finishing;
        Ok(PersistentSourceFactsBuildPoll::Pending)
    }

    fn poll_finish(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<PersistentSourceFactsBuildPoll, SourceFactsAssemblyError> {
        let progress = self.with_resumed_build(arena, |builder, session, receipt| {
            builder.poll_finish(session, receipt)
        })?;
        self.phase = match progress {
            ResumableSequenceProgress::Pending => PersistentSourceFactsBuildPhase::Finishing,
            ResumableSequenceProgress::Complete => PersistentSourceFactsBuildPhase::ReadyForRoot,
        };
        Ok(PersistentSourceFactsBuildPoll::Pending)
    }

    fn take_root(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<PersistentSourceFactsBuildPoll, SourceFactsAssemblyError> {
        let build = self
            .build
            .take()
            .ok_or(SourceFactsAssemblyError::BuilderPoisoned)?;
        arena.validate_suspended_build(&build)?;
        let session = arena.resume_build(build)?;
        let result = self
            .builder
            .as_mut()
            .ok_or(SourceFactsAssemblyError::BuilderPoisoned)?
            .take_root(&session);
        match result {
            Ok(root) => {
                let build = session.suspend()?;
                self.build = Some(build);
                self.build_root = Some(root);
                self.phase = PersistentSourceFactsBuildPhase::ReadyForSeal;
                Ok(PersistentSourceFactsBuildPoll::Pending)
            }
            Err(error) => {
                drop(session);
                self.builder = None;
                self.phase = PersistentSourceFactsBuildPhase::Failed;
                Err(error)
            }
        }
    }

    fn begin_seal(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<PersistentSourceFactsBuildPoll, SourceFactsAssemblyError> {
        let build = self
            .build
            .take()
            .ok_or(SourceFactsAssemblyError::BuilderPoisoned)?;
        let root = self
            .build_root
            .take()
            .ok_or(SourceFactsAssemblyError::BuilderPoisoned)?;
        match begin_measured_sequence_seal(arena, build, root) {
            Ok(seal) => {
                self.builder = None;
                self.seal = Some(seal);
                self.phase = PersistentSourceFactsBuildPhase::Sealing;
                Ok(PersistentSourceFactsBuildPoll::Pending)
            }
            Err(failure) => {
                self.build = Some(failure.build);
                self.build_root = Some(failure.root);
                Err(failure.error.into())
            }
        }
    }

    fn poll_seal(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<PersistentSourceFactsBuildPoll, SourceFactsAssemblyError> {
        let poll = self
            .seal
            .as_mut()
            .ok_or(SourceFactsAssemblyError::BuilderPoisoned)?
            .poll(arena, 1)?;
        self.seal_transitions = self
            .seal_transitions
            .checked_add(poll.transitions)
            .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        let Some(tree) = poll.root else {
            return Ok(PersistentSourceFactsBuildPoll::Pending);
        };
        self.seal = None;
        self.complete_nonempty(arena, tree)
    }

    fn with_resumed_build<T>(
        &mut self,
        arena: &mut PageArena,
        operation: impl FnOnce(
            &mut SourceFactsMeasuredBuilder,
            &mut crate::storage::ArenaBuildSession<'_>,
            &mut SequenceMutationReceipt,
        ) -> Result<T, SourceFactsAssemblyError>,
    ) -> Result<T, SourceFactsAssemblyError> {
        let build = self
            .build
            .take()
            .ok_or(SourceFactsAssemblyError::BuilderPoisoned)?;
        arena.validate_suspended_build(&build)?;
        let mut session = arena.resume_build(build)?;
        let result = operation(
            self.builder
                .as_mut()
                .ok_or(SourceFactsAssemblyError::BuilderPoisoned)?,
            &mut session,
            &mut self.receipt,
        );
        match result {
            Ok(value) => {
                self.build = Some(session.suspend()?);
                Ok(value)
            }
            Err(error) => {
                drop(session);
                self.builder = None;
                self.phase = PersistentSourceFactsBuildPhase::Failed;
                Err(error)
            }
        }
    }

    fn encode_front_page(
        &self,
        output: &mut [u8; ARENA_PAGE_BYTES],
    ) -> Result<usize, SourceFactsAssemblyError> {
        let page = self
            .pending_pages
            .front()
            .ok_or(SourceFactsAssemblyError::BuilderPoisoned)?;
        let profile = self
            .certified
            .as_ref()
            .ok_or(SourceFactsAssemblyError::BuilderPoisoned)?
            .facts
            .profile;
        persistent_sequence_codec::encode_source_fact_sequence_leaf(
            page.canonical(),
            profile,
            output,
        )
    }

    fn finish_front_page(
        &mut self,
        leaf_commitment: persistent_sequence_codec::SourceFactsSequenceCommitment,
    ) {
        let page = self
            .pending_pages
            .pop_front()
            .expect("encoded persistent page remains pending");
        self.completed_pages.push_back(page);
        self.expected_commitment = persistent_sequence_codec::combine_sequence_commitments(
            self.expected_commitment,
            leaf_commitment,
        );
    }

    fn complete_empty(
        &mut self,
    ) -> Result<PersistentSourceFactsBuildPoll, SourceFactsAssemblyError> {
        let mut certified = self
            .certified
            .take()
            .ok_or(SourceFactsAssemblyError::BuilderPoisoned)?;
        if certified.facts.page_count != 0
            || certified.facts.checkpoint_count != 0
            || certified.facts.summary != SourceFactSegmentSummary::default()
            || self.expected_commitment
                != persistent_sequence_codec::SourceFactsSequenceCommitment::empty()
        {
            self.phase = PersistentSourceFactsBuildPhase::Failed;
            return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch);
        }
        certified.facts.pages = std::mem::take(&mut self.completed_pages);
        let root = PersistentSourceFactsRoot {
            source: certified.facts.source,
            parser_profile: certified.parser_profile,
            coverage: certified.coverage,
            profile: certified.facts.profile,
            summary: certified.facts.summary,
            page_count: 0,
            checkpoint_count: 0,
            expected_commitment: self.expected_commitment,
            work: self.work(),
            tree: None,
        };
        self.phase = PersistentSourceFactsBuildPhase::Complete;
        Ok(PersistentSourceFactsBuildPoll::Complete(Box::new(
            PersistentSourceFactsBuildOutput { certified, root },
        )))
    }

    fn complete_nonempty(
        &mut self,
        arena: &mut PageArena,
        tree: SourceFactsMeasuredRoot,
    ) -> Result<PersistentSourceFactsBuildPoll, SourceFactsAssemblyError> {
        let verification = tree.as_ref().summary(arena, &mut self.receipt.inspection);
        let measure = match verification {
            Ok(Some(measure)) => measure,
            Ok(None) => {
                self.release_failed_tree(arena, tree)?;
                self.phase = PersistentSourceFactsBuildPhase::Failed;
                return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch);
            }
            Err(error) => {
                self.release_failed_tree(arena, tree)?;
                self.phase = PersistentSourceFactsBuildPhase::Failed;
                return Err(error);
            }
        };
        let mut certified = self
            .certified
            .take()
            .ok_or(SourceFactsAssemblyError::BuilderPoisoned)?;
        certified.facts.pages = std::mem::take(&mut self.completed_pages);
        let actual = measure.summary();
        if !self.pending_pages.is_empty()
            || certified.facts.pages.len()
                != usize::try_from(certified.facts.page_count)
                    .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?
            || measure.leaves() != certified.facts.page_count
            || actual.checkpoint_spacing_utf16 != certified.facts.profile.checkpoint_spacing_utf16
            || actual.content_fingerprint_algorithm
                != certified.facts.profile.content_fingerprint_algorithm
            || actual.checkpoint_count != certified.facts.checkpoint_count
            || actual.segment != certified.facts.summary
            || actual.commitment != self.expected_commitment
        {
            self.release_failed_tree(arena, tree)?;
            self.phase = PersistentSourceFactsBuildPhase::Failed;
            return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch);
        }
        let root = PersistentSourceFactsRoot {
            source: certified.facts.source,
            parser_profile: certified.parser_profile,
            coverage: certified.coverage,
            profile: certified.facts.profile,
            summary: certified.facts.summary,
            page_count: certified.facts.page_count,
            checkpoint_count: certified.facts.checkpoint_count,
            expected_commitment: self.expected_commitment,
            work: self.work(),
            tree: Some(tree),
        };
        self.phase = PersistentSourceFactsBuildPhase::Complete;
        Ok(PersistentSourceFactsBuildPoll::Complete(Box::new(
            PersistentSourceFactsBuildOutput { certified, root },
        )))
    }

    fn release_failed_tree(
        &self,
        arena: &mut PageArena,
        tree: SourceFactsMeasuredRoot,
    ) -> Result<(), SourceFactsAssemblyError> {
        tree.release(arena)
            .map_err(|failure| SourceFactsAssemblyError::Arena(failure.error))
    }

    pub(crate) fn cancel(&mut self, arena: &mut PageArena) -> Result<(), SourceFactsAssemblyError> {
        if let Some(seal) = self.seal.take() {
            match seal.abort(arena) {
                Ok(()) => {}
                Err(failure) => {
                    self.seal = Some(failure.seal);
                    return Err(failure.error.into());
                }
            }
        }
        if let Some(build) = self.build.as_ref() {
            arena.validate_suspended_build(build)?;
        }
        if let Some(build) = self.build.take() {
            arena.abort_build(build)?;
        }
        self.builder = None;
        self.build_root = None;
        self.phase = PersistentSourceFactsBuildPhase::Failed;
        Ok(())
    }
}

impl Drop for PersistentSourceFactsBuild {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.build.is_none() && self.seal.is_none(),
                "persistent SourceFacts builds require completion or explicit cancellation"
            );
        }
    }
}

/// Strict assembler for one scanner stream over one exact source lease.
///
/// Any malformed page poisons the attempt. Certification consumes the builder,
/// so partial, cancelled, poisoned, or already-completed streams have no API
/// path that can produce [`CertifiedSource`].
pub struct SourceFactsRootBuilder {
    lease: SourceSnapshotLease,
    source: SourceVersion,
    scan_byte_start: usize,
    scan_utf16_start: usize,
    scan_byte_len: usize,
    scan_utf16_len: usize,
    profile: SourceFactsScanProfile,
    parser_profile: ParserProfileId,
    limits: SourceFactsRootLimits,
    admission: SourceFactsRootAdmission,
    scan_id: Option<SourceFactsScanId>,
    next_transport_page_ordinal: u64,
    checkpoint_count: u64,
    checkpoint_hasher: blake3::Hasher,
    pages: LinkedList<Arc<SourceFactRootPage>>,
    partial_page: Vec<SourceFactCheckpoint>,
    canonical_page_base: SourceFactCheckpoint,
    last_checkpoint: Option<SourceFactCheckpoint>,
    terminal_checkpoint_seen: bool,
    poisoned: bool,
}

impl fmt::Debug for SourceFactsRootBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceFactsRootBuilder")
            .field("source", &self.source)
            .field("profile", &self.profile)
            .field("parser_profile", &self.parser_profile)
            .field("limits", &self.limits)
            .field("admission", &self.admission)
            .field("scan_id", &self.scan_id)
            .field(
                "next_transport_page_ordinal",
                &self.next_transport_page_ordinal,
            )
            .field("checkpoint_count", &self.checkpoint_count)
            .field("canonical_page_count", &self.pages.len())
            .field("partial_page_len", &self.partial_page.len())
            .field("terminal_checkpoint_seen", &self.terminal_checkpoint_seen)
            .field("poisoned", &self.poisoned)
            .finish_non_exhaustive()
    }
}

impl SourceFactsRootBuilder {
    /// Starts an unpublished fact root tied to this exact immutable lease.
    pub fn new(
        lease: SourceSnapshotLease,
        profile: SourceFactsScanProfile,
        parser_profile: ParserProfileId,
        limits: SourceFactsRootLimits,
    ) -> Result<Self, SourceFactsAssemblyError> {
        let source = lease.version();
        Self::new_validated_range(
            lease,
            0,
            0,
            source.byte_len(),
            source.utf16_len(),
            profile,
            parser_profile,
            limits,
        )
    }

    pub(crate) fn new_range(
        lease: SourceSnapshotLease,
        range: std::ops::Range<usize>,
        profile: SourceFactsScanProfile,
        parser_profile: ParserProfileId,
        limits: SourceFactsRootLimits,
    ) -> Result<Self, SourceFactsAssemblyError> {
        if range.start > range.end || range.end > lease.version().byte_len() {
            return Err(SourceFactsAssemblyError::SourceDimensionMismatch);
        }
        let utf16_start = lease
            .utf16_offset_for_byte(range.start)
            .map_err(|_| SourceFactsAssemblyError::CheckpointCoordinateMismatch)?;
        let utf16_end = lease
            .utf16_offset_for_byte(range.end)
            .map_err(|_| SourceFactsAssemblyError::CheckpointCoordinateMismatch)?;
        Self::new_validated_range(
            lease,
            range.start,
            utf16_start,
            range.end - range.start,
            utf16_end - utf16_start,
            profile,
            parser_profile,
            limits,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_validated_range(
        lease: SourceSnapshotLease,
        scan_byte_start: usize,
        scan_utf16_start: usize,
        scan_byte_len: usize,
        scan_utf16_len: usize,
        profile: SourceFactsScanProfile,
        parser_profile: ParserProfileId,
        limits: SourceFactsRootLimits,
    ) -> Result<Self, SourceFactsAssemblyError> {
        let source = lease.version();
        let admission = SourceFactsRootAdmission::for_utf16_len(scan_utf16_len, profile, limits)?;
        let mut partial_page = Vec::new();
        partial_page
            .try_reserve_exact(
                admission
                    .checkpoint_count
                    .min(SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX),
            )
            .map_err(|_| SourceFactsAssemblyError::AllocationFailed)?;
        Ok(Self {
            lease,
            source,
            scan_byte_start,
            scan_utf16_start,
            scan_byte_len,
            scan_utf16_len,
            profile,
            parser_profile,
            limits,
            admission,
            scan_id: None,
            next_transport_page_ordinal: 0,
            checkpoint_count: 0,
            checkpoint_hasher: checkpoint_sequence_hasher(profile.checkpoint_spacing_utf16),
            pages: LinkedList::new(),
            partial_page,
            canonical_page_base: SourceFactCheckpoint::default(),
            last_checkpoint: None,
            terminal_checkpoint_seen: false,
            poisoned: false,
        })
    }

    /// Accepts exactly the next non-empty bounded checkpoint page.
    pub fn push_page(
        &mut self,
        page: SourceFactCheckpointPage,
    ) -> Result<(), SourceFactsAssemblyError> {
        if self.poisoned {
            return Err(SourceFactsAssemblyError::BuilderPoisoned);
        }
        let result = self.push_page_inner(page);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    fn push_page_inner(
        &mut self,
        page: SourceFactCheckpointPage,
    ) -> Result<(), SourceFactsAssemblyError> {
        if self.terminal_checkpoint_seen {
            return Err(SourceFactsAssemblyError::PageAfterTerminalCheckpoint);
        }
        if page.source != self.source {
            return Err(SourceFactsAssemblyError::SourceMismatch);
        }
        if page.checkpoint_spacing_utf16 != self.profile.checkpoint_spacing_utf16 {
            return Err(SourceFactsAssemblyError::ProfileMismatch);
        }
        self.bind_scan(page.scan_id)?;
        if page.ordinal != self.next_transport_page_ordinal {
            return Err(SourceFactsAssemblyError::UnexpectedPageOrdinal {
                expected: self.next_transport_page_ordinal,
                actual: page.ordinal,
            });
        }
        if page.checkpoints.is_empty() {
            return Err(SourceFactsAssemblyError::EmptyCheckpointPage);
        }
        if page.checkpoints.len() > SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX {
            return Err(SourceFactsAssemblyError::CheckpointPageTooLarge {
                observed: page.checkpoints.len(),
                limit: SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX,
            });
        }
        self.enforce_assembly_admission(page.checkpoints.len())?;
        let page_last_index = page
            .checkpoints
            .len()
            .checked_sub(1)
            .ok_or(SourceFactsAssemblyError::EmptyCheckpointPage)?;
        for (index, checkpoint) in page.checkpoints.iter().copied().enumerate() {
            self.validate_checkpoint(checkpoint, index == page_last_index)?;
            append_checkpoint_to_hasher(&mut self.checkpoint_hasher, checkpoint);
            self.last_checkpoint = Some(checkpoint);
            self.checkpoint_count = self
                .checkpoint_count
                .checked_add(1)
                .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
            self.append_canonical_checkpoint(checkpoint)?;
        }
        if checkpoint_sequence_digest(&self.checkpoint_hasher) != page.sequence_digest {
            return Err(SourceFactsAssemblyError::PageDigestMismatch);
        }
        self.next_transport_page_ordinal = self
            .next_transport_page_ordinal
            .checked_add(1)
            .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        Ok(())
    }

    fn enforce_assembly_admission(
        &self,
        incoming_checkpoints: usize,
    ) -> Result<(), SourceFactsAssemblyError> {
        let current = usize::try_from(self.checkpoint_count)
            .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?;
        let needed_checkpoints = current
            .checked_add(incoming_checkpoints)
            .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        if needed_checkpoints > self.admission.checkpoint_count {
            return Err(SourceFactsAssemblyError::AdmissionCheckpointLimitExceeded {
                needed: needed_checkpoints,
                limit: self
                    .admission
                    .checkpoint_count
                    .min(self.limits.max_checkpoints),
            });
        }
        let needed_pages = needed_checkpoints.div_ceil(SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX);
        if needed_pages > self.admission.page_count || needed_pages > self.limits.max_pages {
            return Err(SourceFactsAssemblyError::AdmissionPageLimitExceeded {
                needed: needed_pages,
                limit: self.admission.page_count.min(self.limits.max_pages),
            });
        }
        let resident_bytes = source_facts_resident_bytes(needed_checkpoints, needed_pages)?;
        if resident_bytes > self.limits.max_resident_bytes {
            return Err(
                SourceFactsAssemblyError::AdmissionResidentBytesLimitExceeded {
                    needed: resident_bytes,
                    limit: self.limits.max_resident_bytes,
                },
            );
        }
        Ok(())
    }

    fn append_canonical_checkpoint(
        &mut self,
        checkpoint: SourceFactCheckpoint,
    ) -> Result<(), SourceFactsAssemblyError> {
        self.partial_page.push(checkpoint);
        if self.partial_page.len() == SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX {
            self.flush_canonical_page()?;
        }
        Ok(())
    }

    fn flush_canonical_page(&mut self) -> Result<(), SourceFactsAssemblyError> {
        if self.partial_page.is_empty() {
            return Ok(());
        }
        let mut absolute_checkpoints = Vec::new();
        std::mem::swap(&mut absolute_checkpoints, &mut self.partial_page);
        let terminal = absolute_checkpoints
            .last()
            .copied()
            .ok_or(SourceFactsAssemblyError::EmptyCheckpointPage)?;
        let (starts_with_line_feed, ends_with_carriage_return) =
            self.page_boundary_flags(self.canonical_page_base.byte_offset, terminal.byte_offset)?;
        let mut relative_checkpoints = Vec::new();
        relative_checkpoints
            .try_reserve_exact(absolute_checkpoints.len())
            .map_err(|_| SourceFactsAssemblyError::AllocationFailed)?;
        for checkpoint in absolute_checkpoints.iter().copied() {
            relative_checkpoints.push(SourceFactRelativeCheckpoint {
                summary: relative_checkpoint_summary(
                    self.canonical_page_base,
                    checkpoint,
                    starts_with_line_feed,
                    ends_with_carriage_return && checkpoint.byte_offset == terminal.byte_offset,
                )?,
            });
        }
        let summary = relative_checkpoints
            .last()
            .copied()
            .ok_or(SourceFactsAssemblyError::EmptyCheckpointPage)?
            .summary;
        let content_digest = source_fact_page_digest(
            self.profile.checkpoint_spacing_utf16,
            summary,
            &relative_checkpoints,
        );
        let canonical = Arc::new(SourceFactCanonicalPage {
            content_digest,
            summary,
            checkpoints: relative_checkpoints.into_boxed_slice(),
        });
        self.pages.push_back(Arc::new(SourceFactRootPage {
            canonical,
            absolute_checkpoints: absolute_checkpoints.into_boxed_slice(),
        }));
        self.canonical_page_base = terminal;

        let assembled = usize::try_from(self.checkpoint_count)
            .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?;
        let remaining = self.admission.checkpoint_count.saturating_sub(assembled);
        self.partial_page
            .try_reserve_exact(remaining.min(SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX))
            .map_err(|_| SourceFactsAssemblyError::AllocationFailed)?;
        Ok(())
    }

    fn page_boundary_flags(
        &self,
        start: u64,
        end: u64,
    ) -> Result<(bool, bool), SourceFactsAssemblyError> {
        let local_start =
            usize::try_from(start).map_err(|_| SourceFactsAssemblyError::PageBoundaryReadFailed)?;
        let local_end =
            usize::try_from(end).map_err(|_| SourceFactsAssemblyError::PageBoundaryReadFailed)?;
        let start = self
            .scan_byte_start
            .checked_add(local_start)
            .ok_or(SourceFactsAssemblyError::PageBoundaryReadFailed)?;
        let end = self
            .scan_byte_start
            .checked_add(local_end)
            .ok_or(SourceFactsAssemblyError::PageBoundaryReadFailed)?;
        if start >= end {
            return Err(SourceFactsAssemblyError::PageBoundaryReadFailed);
        }
        let mut cursor = self
            .lease
            .duplicate()
            .cursor_in(start..end)
            .map_err(|_| SourceFactsAssemblyError::PageBoundaryReadFailed)?;
        let mut first = [0_u8; 1];
        if cursor.read(&mut first) != 1 {
            return Err(SourceFactsAssemblyError::PageBoundaryReadFailed);
        }
        let starts_with_line_feed = first[0] == b'\n';
        let final_byte_start = end
            .checked_sub(1)
            .ok_or(SourceFactsAssemblyError::PageBoundaryReadFailed)?;
        let ends_with_carriage_return =
            match self.lease.duplicate().cursor_in(final_byte_start..end) {
                Ok(mut cursor) => {
                    let mut final_byte = [0_u8; 1];
                    if cursor.read(&mut final_byte) != 1 {
                        return Err(SourceFactsAssemblyError::PageBoundaryReadFailed);
                    }
                    final_byte[0] == b'\r'
                }
                Err(SourceEditError::SplitUtf8Scalar { .. }) => false,
                Err(_) => return Err(SourceFactsAssemblyError::PageBoundaryReadFailed),
            };
        Ok((starts_with_line_feed, ends_with_carriage_return))
    }

    fn validate_checkpoint(
        &mut self,
        checkpoint: SourceFactCheckpoint,
        is_last_in_page: bool,
    ) -> Result<(), SourceFactsAssemblyError> {
        let byte_len = u64::try_from(self.scan_byte_len)
            .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?;
        let utf16_len = u64::try_from(self.scan_utf16_len)
            .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?;
        if checkpoint.byte_offset == 0
            || checkpoint.utf16_offset == 0
            || checkpoint.byte_offset > byte_len
            || checkpoint.utf16_offset > utf16_len
            || checkpoint.logical_line_breaks > checkpoint.utf16_offset
        {
            return Err(SourceFactsAssemblyError::CheckpointOutOfBounds);
        }

        let (prior_byte, prior_utf16, prior_lines) =
            self.last_checkpoint.map_or((0, 0, 0), |prior| {
                (
                    prior.byte_offset,
                    prior.utf16_offset,
                    prior.logical_line_breaks,
                )
            });
        if checkpoint.byte_offset <= prior_byte || checkpoint.utf16_offset <= prior_utf16 {
            return Err(SourceFactsAssemblyError::CheckpointNotMonotonic);
        }
        if checkpoint.logical_line_breaks < prior_lines {
            return Err(SourceFactsAssemblyError::LogicalLineBreakRegression);
        }
        let maximum_gap = self
            .profile
            .checkpoint_spacing_utf16
            .checked_add(1)
            .ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        if checkpoint
            .utf16_offset
            .checked_sub(prior_utf16)
            .is_none_or(|gap| gap > maximum_gap)
        {
            return Err(SourceFactsAssemblyError::CheckpointCoverageGap);
        }

        let checkpoint_utf16 = usize::try_from(checkpoint.utf16_offset)
            .map_err(|_| SourceFactsAssemblyError::CheckpointOutOfBounds)?;
        let absolute_utf16 = self
            .scan_utf16_start
            .checked_add(checkpoint_utf16)
            .ok_or(SourceFactsAssemblyError::CheckpointCoordinateMismatch)?;
        let expected_byte = self
            .lease
            .byte_offset_for_utf16(absolute_utf16)
            .map_err(|_| SourceFactsAssemblyError::CheckpointCoordinateMismatch)?;
        let expected_local_byte = expected_byte
            .checked_sub(self.scan_byte_start)
            .ok_or(SourceFactsAssemblyError::CheckpointCoordinateMismatch)?;
        if u64::try_from(expected_local_byte).ok() != Some(checkpoint.byte_offset) {
            return Err(SourceFactsAssemblyError::CheckpointCoordinateMismatch);
        }

        let reaches_terminal =
            checkpoint.byte_offset == byte_len || checkpoint.utf16_offset == utf16_len;
        if reaches_terminal {
            if checkpoint.byte_offset != byte_len || checkpoint.utf16_offset != utf16_len {
                return Err(SourceFactsAssemblyError::CheckpointOutOfBounds);
            }
            if !is_last_in_page {
                return Err(SourceFactsAssemblyError::TerminalCheckpointNotLast);
            }
            self.terminal_checkpoint_seen = true;
        }
        Ok(())
    }

    fn bind_scan(&mut self, scan_id: SourceFactsScanId) -> Result<(), SourceFactsAssemblyError> {
        match self.scan_id {
            Some(expected) if expected != scan_id => Err(SourceFactsAssemblyError::ScanMismatch),
            Some(_) => Ok(()),
            None => {
                self.scan_id = Some(scan_id);
                Ok(())
            }
        }
    }

    /// Validates clean EOF and consumes the only path to certification.
    pub fn certify(
        self,
        completion: SourceFactsCompletion,
    ) -> Result<CertifiedSource, SourceFactsAssemblyError> {
        if self.scan_byte_start != 0
            || self.scan_utf16_start != 0
            || self.scan_byte_len != self.source.byte_len()
            || self.scan_utf16_len != self.source.utf16_len()
        {
            return Err(SourceFactsAssemblyError::SourceDimensionMismatch);
        }
        let (lease, parser_profile, facts) = self.finish_root(completion)?;
        Ok(CertifiedSource {
            lease,
            parser_profile,
            facts,
            coverage: SourceFactsCoverage::CleanEof,
        })
    }

    pub(crate) fn finish_segment(
        self,
        completion: SourceFactsCompletion,
    ) -> Result<CertifiedSource, SourceFactsAssemblyError> {
        let byte_start = u64::try_from(self.scan_byte_start)
            .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?;
        let byte_end = u64::try_from(
            self.scan_byte_start
                .checked_add(self.scan_byte_len)
                .ok_or(SourceFactsAssemblyError::CounterExhausted)?,
        )
        .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?;
        let (lease, parser_profile, facts) = self.finish_root(completion)?;
        Ok(CertifiedSource {
            lease,
            parser_profile,
            facts,
            coverage: SourceFactsCoverage::ExactRange {
                byte_start,
                byte_end,
            },
        })
    }

    fn finish_root(
        mut self,
        completion: SourceFactsCompletion,
    ) -> Result<(SourceSnapshotLease, ParserProfileId, SourceFactsRoot), SourceFactsAssemblyError>
    {
        if self.poisoned {
            return Err(SourceFactsAssemblyError::BuilderPoisoned);
        }
        if completion.source != self.source {
            return Err(SourceFactsAssemblyError::SourceMismatch);
        }
        if completion.checkpoint_spacing_utf16 != self.profile.checkpoint_spacing_utf16 {
            return Err(SourceFactsAssemblyError::ProfileMismatch);
        }
        self.bind_scan(completion.scan_id)?;
        if completion.page_count != self.next_transport_page_ordinal {
            return Err(SourceFactsAssemblyError::PageCountMismatch);
        }
        if completion.checkpoint_count != self.checkpoint_count {
            return Err(SourceFactsAssemblyError::CheckpointCountMismatch);
        }
        let recomputed_digest = checkpoint_sequence_digest(&self.checkpoint_hasher);
        if completion.checkpoint_sequence_digest != recomputed_digest {
            return Err(SourceFactsAssemblyError::CompletionDigestMismatch);
        }
        if completion.fingerprint.algorithm != self.profile.content_fingerprint_algorithm {
            return Err(SourceFactsAssemblyError::FingerprintAlgorithmMismatch);
        }
        let expected_bytes = u64::try_from(self.scan_byte_len)
            .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?;
        let expected_utf16 = u64::try_from(self.scan_utf16_len)
            .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?;
        if completion.fingerprint.byte_len != expected_bytes
            || completion.fingerprint.utf16_len != expected_utf16
        {
            return Err(SourceFactsAssemblyError::SourceDimensionMismatch);
        }

        if expected_bytes == 0 && expected_utf16 == 0 {
            if self.last_checkpoint.is_some()
                || self.terminal_checkpoint_seen
                || !self.partial_page.is_empty()
                || completion.page_count != 0
                || completion.checkpoint_count != 0
                || completion.logical_line_breaks != 0
                || completion.fingerprint.rolling_hash != SourceContentHash128::default()
            {
                return Err(SourceFactsAssemblyError::EmptySourceFactsMismatch);
            }
        } else {
            let terminal = self
                .last_checkpoint
                .ok_or(SourceFactsAssemblyError::MissingTerminalCheckpoint)?;
            if !self.terminal_checkpoint_seen
                || terminal.byte_offset != expected_bytes
                || terminal.utf16_offset != expected_utf16
            {
                return Err(SourceFactsAssemblyError::MissingTerminalCheckpoint);
            }
            if terminal.rolling_hash != completion.fingerprint.rolling_hash {
                return Err(SourceFactsAssemblyError::TerminalFingerprintMismatch);
            }
            if terminal.logical_line_breaks != completion.logical_line_breaks {
                return Err(SourceFactsAssemblyError::TerminalLineBreakMismatch);
            }
        }

        // Transport poll boundaries are not storage authority. Only after the
        // complete scanner stream has passed every clean-EOF check do we seal
        // the one canonical tail page.
        self.flush_canonical_page()?;
        let canonical_page_count = u64::try_from(self.pages.len())
            .map_err(|_| SourceFactsAssemblyError::CounterExhausted)?;
        let summary = self
            .pages
            .iter()
            .try_fold(SourceFactSegmentSummary::default(), |prefix, page| {
                prefix.checked_followed_by(page.canonical.summary)
            });
        let summary = summary.ok_or(SourceFactsAssemblyError::CounterExhausted)?;
        if summary.byte_len != completion.fingerprint.byte_len
            || summary.utf16_len != completion.fingerprint.utf16_len
            || summary.logical_line_breaks != completion.logical_line_breaks
            || summary.rolling_hash != completion.fingerprint.rolling_hash
        {
            return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch);
        }

        let scan_id = self.scan_id.ok_or(SourceFactsAssemblyError::ScanMismatch)?;
        let facts = SourceFactsRoot {
            source: self.source,
            profile: self.profile,
            scan_id,
            sequence_digest: recomputed_digest,
            summary,
            page_count: canonical_page_count,
            checkpoint_count: completion.checkpoint_count,
            pages: std::mem::take(&mut self.pages),
        };
        Ok((self.lease, self.parser_profile, facts))
    }
}

fn checkpoint_sequence_hasher(checkpoint_spacing_utf16: u64) -> blake3::Hasher {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.source-facts.checkpoints.v1\0");
    hasher.update(&checkpoint_spacing_utf16.to_le_bytes());
    hasher
}

fn append_checkpoint_to_hasher(hasher: &mut blake3::Hasher, checkpoint: SourceFactCheckpoint) {
    hasher.update(b"flark.source-facts.checkpoint.v1\0");
    hasher.update(&checkpoint.byte_offset.to_le_bytes());
    hasher.update(&checkpoint.utf16_offset.to_le_bytes());
    hasher.update(&checkpoint.logical_line_breaks.to_le_bytes());
    for word in checkpoint.rolling_hash.words {
        hasher.update(&word.to_le_bytes());
    }
}

fn checkpoint_sequence_digest(hasher: &blake3::Hasher) -> SourceFactSequenceDigest {
    SourceFactSequenceDigest(*hasher.clone().finalize().as_bytes())
}

fn relative_checkpoint_summary(
    base: SourceFactCheckpoint,
    checkpoint: SourceFactCheckpoint,
    starts_with_line_feed: bool,
    ends_with_carriage_return: bool,
) -> Result<SourceFactSegmentSummary, SourceFactsAssemblyError> {
    let byte_len = checkpoint
        .byte_offset
        .checked_sub(base.byte_offset)
        .ok_or(SourceFactsAssemblyError::CheckpointNotMonotonic)?;
    let utf16_len = checkpoint
        .utf16_offset
        .checked_sub(base.utf16_offset)
        .ok_or(SourceFactsAssemblyError::CheckpointNotMonotonic)?;
    let logical_line_breaks = checkpoint
        .logical_line_breaks
        .checked_sub(base.logical_line_breaks)
        .ok_or(SourceFactsAssemblyError::LogicalLineBreakRegression)?;
    Ok(SourceFactSegmentSummary {
        byte_len,
        utf16_len,
        logical_line_breaks,
        rolling_hash: checkpoint
            .rolling_hash
            .suffix_after(base.rolling_hash, byte_len),
        starts_with_line_feed,
        ends_with_carriage_return,
    })
}

fn source_fact_page_digest(
    checkpoint_spacing_utf16: u64,
    summary: SourceFactSegmentSummary,
    checkpoints: &[SourceFactRelativeCheckpoint],
) -> SourceFactPageDigest {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.source-facts.root-page.v2\0");
    hasher.update(&checkpoint_spacing_utf16.to_le_bytes());
    append_segment_summary_to_hasher(&mut hasher, summary);
    for checkpoint in checkpoints.iter().copied() {
        hasher.update(b"flark.source-facts.relative-checkpoint.v1\0");
        append_segment_summary_to_hasher(&mut hasher, checkpoint.summary);
    }
    SourceFactPageDigest(*hasher.finalize().as_bytes())
}

fn append_segment_summary_to_hasher(
    hasher: &mut blake3::Hasher,
    summary: SourceFactSegmentSummary,
) {
    hasher.update(&summary.byte_len.to_le_bytes());
    hasher.update(&summary.utf16_len.to_le_bytes());
    hasher.update(&summary.logical_line_breaks.to_le_bytes());
    for word in summary.rolling_hash.words {
        hasher.update(&word.to_le_bytes());
    }
    hasher.update(&[
        u8::from(summary.starts_with_line_feed),
        u8::from(summary.ends_with_carriage_return),
    ]);
}

/// Fuel-bounded scanner over one exact immutable source snapshot.
pub struct SourceFactsScanner {
    scan_id: SourceFactsScanId,
    source: SourceVersion,
    scan_byte_len: u64,
    scan_utf16_len: u64,
    cursor: Option<SourceCursor>,
    checkpoint_spacing_utf16: u64,
    next_checkpoint_utf16: u64,
    byte_offset: u64,
    utf16_offset: u64,
    logical_line_breaks: u64,
    hash: SourceContentHash128,
    checkpoint_hasher: blake3::Hasher,
    pending_utf8_continuations: u8,
    pending_carriage_return: bool,
    read_buffer: [u8; SOURCE_CURSOR_WINDOW_BYTES],
    read_start: usize,
    read_end: usize,
    cursor_bytes_read: usize,
    next_page_ordinal: u64,
    checkpoint_count: u64,
    page_count: u64,
    final_checkpoint_emitted: bool,
    complete: bool,
    cancelled: bool,
    poisoned: bool,
}

impl fmt::Debug for SourceFactsScanner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceFactsScanner")
            .field("scan_id", &self.scan_id)
            .field("source", &self.source)
            .field("scan_byte_len", &self.scan_byte_len)
            .field("scan_utf16_len", &self.scan_utf16_len)
            .field("byte_offset", &self.byte_offset)
            .field("utf16_offset", &self.utf16_offset)
            .field("checkpoint_count", &self.checkpoint_count)
            .field("page_count", &self.page_count)
            .field("complete", &self.complete)
            .field("cancelled", &self.cancelled)
            .field("poisoned", &self.poisoned)
            .finish_non_exhaustive()
    }
}

impl SourceFactsScanner {
    /// Starts a scan. Spacing is measured in UTF-16 code units.
    pub fn new(
        source: SourceSnapshotLease,
        checkpoint_spacing_utf16: usize,
    ) -> Result<Self, SourceFactsError> {
        let profile = SourceFactsScanProfile::new(checkpoint_spacing_utf16)?;
        Self::with_profile(source, profile)
    }

    /// Starts a scan under one previously validated profile.
    pub fn with_profile(
        source: SourceSnapshotLease,
        profile: SourceFactsScanProfile,
    ) -> Result<Self, SourceFactsError> {
        let byte_len = source.version().byte_len();
        Self::with_profile_range(source, profile, 0..byte_len)
    }

    /// Starts the same scanner over one exact scalar-aligned source crop.
    /// Counters and checkpoints are segment-local; `source` remains the full
    /// immutable authority so a crop cannot be crossed between revisions.
    pub(crate) fn with_profile_range(
        source: SourceSnapshotLease,
        profile: SourceFactsScanProfile,
        range: std::ops::Range<usize>,
    ) -> Result<Self, SourceFactsError> {
        validate_scan_profile(profile)?;
        let version = source.version();
        let start_utf16 = source.utf16_offset_for_byte(range.start)?;
        let end_utf16 = source.utf16_offset_for_byte(range.end)?;
        let scan_byte_len = u64::try_from(range.end.checked_sub(range.start).ok_or(
            SourceFactsError::Source(SourceEditError::InvalidRange {
                start: range.start,
                end: range.end,
                len: version.byte_len(),
            }),
        )?)
        .map_err(|_| SourceFactsError::CounterExhausted)?;
        let scan_utf16_len = u64::try_from(
            end_utf16
                .checked_sub(start_utf16)
                .ok_or(SourceFactsError::CounterExhausted)?,
        )
        .map_err(|_| SourceFactsError::CounterExhausted)?;
        let cursor = source.cursor_in(range)?;
        let scan_id = SourceFactsScanId::allocate()?;
        let spacing = profile.checkpoint_spacing_utf16;
        Ok(Self {
            scan_id,
            source: version,
            scan_byte_len,
            scan_utf16_len,
            cursor: Some(cursor),
            checkpoint_spacing_utf16: spacing,
            next_checkpoint_utf16: spacing,
            byte_offset: 0,
            utf16_offset: 0,
            logical_line_breaks: 0,
            hash: SourceContentHash128::default(),
            checkpoint_hasher: checkpoint_sequence_hasher(spacing),
            pending_utf8_continuations: 0,
            pending_carriage_return: false,
            read_buffer: [0; SOURCE_CURSOR_WINDOW_BYTES],
            read_start: 0,
            read_end: 0,
            cursor_bytes_read: 0,
            next_page_ordinal: 0,
            checkpoint_count: 0,
            page_count: 0,
            final_checkpoint_emitted: scan_byte_len == 0,
            complete: false,
            cancelled: false,
            poisoned: false,
        })
    }

    /// Returns the exact profile that shapes this scan.
    #[must_use]
    pub const fn profile(&self) -> SourceFactsScanProfile {
        SourceFactsScanProfile {
            checkpoint_spacing_utf16: self.checkpoint_spacing_utf16,
            content_fingerprint_algorithm: SOURCE_CONTENT_FINGERPRINT_ALGORITHM,
        }
    }

    /// Cancels by dropping the only scanner-owned cursor and source lease.
    pub fn cancel(&mut self) -> bool {
        if self.cancelled || self.complete || self.poisoned {
            return false;
        }
        self.cancelled = true;
        self.cursor = None;
        self.read_start = 0;
        self.read_end = 0;
        true
    }

    fn poison(&mut self) {
        self.poisoned = true;
        self.cursor = None;
        self.read_start = 0;
        self.read_end = 0;
    }

    /// Advances by at most `maximum_source_bytes` and emits at most the
    /// requested number of checkpoints.
    pub fn poll(
        &mut self,
        maximum_source_bytes: usize,
        maximum_checkpoints: usize,
    ) -> Result<SourceFactsPoll, SourceFactsError> {
        if self.cancelled {
            return Ok(SourceFactsPoll::Cancelled);
        }
        if self.complete {
            return Err(SourceFactsError::AlreadyComplete);
        }
        if self.poisoned {
            return Err(SourceFactsError::ScannerPoisoned);
        }
        if maximum_source_bytes == 0 || maximum_checkpoints == 0 {
            return Err(SourceFactsError::ZeroFuel);
        }
        if maximum_source_bytes > SOURCE_FACT_SOURCE_BYTES_PER_POLL_MAX
            || maximum_checkpoints > SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX
        {
            return Err(SourceFactsError::PollLimitExceeded);
        }

        let result = self.poll_inner(maximum_source_bytes, maximum_checkpoints);
        if result.is_err() {
            self.poison();
        }
        result
    }

    fn poll_inner(
        &mut self,
        maximum_source_bytes: usize,
        maximum_checkpoints: usize,
    ) -> Result<SourceFactsPoll, SourceFactsError> {
        let mut checkpoints = Vec::new();
        checkpoints
            .try_reserve_exact(maximum_checkpoints)
            .map_err(|_| SourceFactsError::AllocationFailed)?;
        let buffered_before = self.cursor_bytes_read;
        let refills_before = self.cursor_refill_count();
        let mut examined = 0;
        while examined < maximum_source_bytes && checkpoints.len() < maximum_checkpoints {
            let Some(byte) = self.next_buffered_byte(maximum_source_bytes - examined)? else {
                break;
            };
            if let Some(checkpoint) = self.consume_byte(byte)? {
                checkpoints.push(checkpoint);
            }
            examined += 1;
            if checkpoints.len() < maximum_checkpoints
                && self.at_safe_checkpoint_boundary()
                && self.utf16_offset >= self.next_checkpoint_utf16
            {
                let checkpoint = self.checkpoint();
                self.record_checkpoint(checkpoint);
                checkpoints.push(checkpoint);
                if self.byte_offset == self.scan_byte_len {
                    self.final_checkpoint_emitted = true;
                }
                self.advance_checkpoint_target()?;
            }
        }

        let at_eof = self.read_start == self.read_end
            && self
                .cursor
                .as_ref()
                .is_some_and(|cursor| cursor.position() == cursor.end());
        if at_eof {
            self.finish_stream()?;
            if !self.final_checkpoint_emitted && checkpoints.len() < maximum_checkpoints {
                let checkpoint = self.checkpoint();
                self.record_checkpoint(checkpoint);
                checkpoints.push(checkpoint);
                self.final_checkpoint_emitted = true;
            }
        }

        let work =
            self.work_receipt(examined, checkpoints.len(), buffered_before, refills_before)?;
        if !checkpoints.is_empty() {
            let checkpoint_len =
                u64::try_from(checkpoints.len()).map_err(|_| SourceFactsError::CounterExhausted)?;
            self.checkpoint_count = self
                .checkpoint_count
                .checked_add(checkpoint_len)
                .ok_or(SourceFactsError::CounterExhausted)?;
            let ordinal = self.next_page_ordinal;
            self.next_page_ordinal = self
                .next_page_ordinal
                .checked_add(1)
                .ok_or(SourceFactsError::CounterExhausted)?;
            self.page_count = self
                .page_count
                .checked_add(1)
                .ok_or(SourceFactsError::CounterExhausted)?;
            return Ok(SourceFactsPoll::Page {
                page: SourceFactCheckpointPage {
                    scan_id: self.scan_id,
                    source: self.source,
                    checkpoint_spacing_utf16: self.checkpoint_spacing_utf16,
                    ordinal,
                    sequence_digest: self.checkpoint_sequence_digest(),
                    checkpoints: checkpoints.into_boxed_slice(),
                },
                work,
            });
        }

        if at_eof && self.final_checkpoint_emitted {
            self.complete = true;
            self.cursor = None;
            return Ok(SourceFactsPoll::Complete {
                completion: self.completion()?,
                work,
            });
        }
        Ok(SourceFactsPoll::Pending(work))
    }

    fn cursor_refill_count(&self) -> usize {
        self.cursor.as_ref().map_or(0, SourceCursor::refill_count)
    }

    fn work_receipt(
        &self,
        examined: usize,
        checkpoints: usize,
        buffered_before: usize,
        refills_before: usize,
    ) -> Result<SourceFactsWork, SourceFactsError> {
        let source_bytes_buffered = self
            .cursor_bytes_read
            .checked_sub(buffered_before)
            .ok_or(SourceFactsError::CounterExhausted)?;
        let cursor_refills = self
            .cursor_refill_count()
            .checked_sub(refills_before)
            .ok_or(SourceFactsError::CounterExhausted)?;
        let cursor_copy_bytes_upper_bound = cursor_refills
            .checked_mul(SOURCE_CURSOR_WINDOW_BYTES)
            .ok_or(SourceFactsError::CounterExhausted)?;
        Ok(SourceFactsWork {
            source_bytes_examined: examined,
            source_bytes_buffered,
            cursor_refills,
            cursor_copy_bytes_upper_bound,
            checkpoints_emitted: checkpoints,
        })
    }

    fn next_buffered_byte(
        &mut self,
        remaining_fuel: usize,
    ) -> Result<Option<u8>, SourceFactsError> {
        if self.read_start == self.read_end {
            let cursor = self
                .cursor
                .as_mut()
                .ok_or(SourceFactsError::CorruptSource("source cursor disappeared"))?;
            let requested = remaining_fuel.min(self.read_buffer.len());
            self.read_end = cursor.read(&mut self.read_buffer[..requested]);
            self.read_start = 0;
            self.cursor_bytes_read = self
                .cursor_bytes_read
                .checked_add(self.read_end)
                .ok_or(SourceFactsError::CounterExhausted)?;
            if self.read_end == 0 {
                return Ok(None);
            }
        }
        let byte = self.read_buffer[self.read_start];
        self.read_start += 1;
        Ok(Some(byte))
    }

    fn consume_byte(&mut self, byte: u8) -> Result<Option<SourceFactCheckpoint>, SourceFactsError> {
        let followed_carriage_return = self.pending_carriage_return;
        let mut proven_prefix_checkpoint = None;
        if followed_carriage_return {
            self.logical_line_breaks = self
                .logical_line_breaks
                .checked_add(1)
                .ok_or(SourceFactsError::CounterExhausted)?;
            self.pending_carriage_return = false;
            if byte != b'\n' && self.utf16_offset >= self.next_checkpoint_utf16 {
                let checkpoint = self.checkpoint();
                self.record_checkpoint(checkpoint);
                self.advance_checkpoint_target()?;
                proven_prefix_checkpoint = Some(checkpoint);
            }
        }

        self.hash.append_byte(byte);
        self.byte_offset = self
            .byte_offset
            .checked_add(1)
            .ok_or(SourceFactsError::CounterExhausted)?;
        match byte {
            b'\r' => self.pending_carriage_return = true,
            b'\n' if !followed_carriage_return => {
                self.logical_line_breaks = self
                    .logical_line_breaks
                    .checked_add(1)
                    .ok_or(SourceFactsError::CounterExhausted)?;
            }
            _ => {}
        }
        self.consume_utf8_shape(byte)?;
        Ok(proven_prefix_checkpoint)
    }

    fn consume_utf8_shape(&mut self, byte: u8) -> Result<(), SourceFactsError> {
        if self.pending_utf8_continuations > 0 {
            if !(0x80..=0xbf).contains(&byte) {
                return Err(SourceFactsError::CorruptSource(
                    "invalid UTF-8 continuation",
                ));
            }
            self.pending_utf8_continuations -= 1;
            return Ok(());
        }
        let (utf16_units, continuations) = match byte {
            0x00..=0x7f => (1_u64, 0),
            0xc2..=0xdf => (1, 1),
            0xe0..=0xef => (1, 2),
            0xf0..=0xf4 => (2, 3),
            _ => return Err(SourceFactsError::CorruptSource("invalid UTF-8 lead byte")),
        };
        self.utf16_offset = self
            .utf16_offset
            .checked_add(utf16_units)
            .ok_or(SourceFactsError::CounterExhausted)?;
        self.pending_utf8_continuations = continuations;
        Ok(())
    }

    fn at_safe_checkpoint_boundary(&self) -> bool {
        self.pending_utf8_continuations == 0 && !self.pending_carriage_return
    }

    fn checkpoint(&self) -> SourceFactCheckpoint {
        SourceFactCheckpoint {
            byte_offset: self.byte_offset,
            utf16_offset: self.utf16_offset,
            logical_line_breaks: self.logical_line_breaks,
            rolling_hash: self.hash,
        }
    }

    fn record_checkpoint(&mut self, checkpoint: SourceFactCheckpoint) {
        append_checkpoint_to_hasher(&mut self.checkpoint_hasher, checkpoint);
    }

    fn checkpoint_sequence_digest(&self) -> SourceFactSequenceDigest {
        checkpoint_sequence_digest(&self.checkpoint_hasher)
    }

    fn advance_checkpoint_target(&mut self) -> Result<(), SourceFactsError> {
        while self.next_checkpoint_utf16 <= self.utf16_offset {
            self.next_checkpoint_utf16 = self
                .next_checkpoint_utf16
                .checked_add(self.checkpoint_spacing_utf16)
                .ok_or(SourceFactsError::CounterExhausted)?;
        }
        Ok(())
    }

    fn finish_stream(&mut self) -> Result<(), SourceFactsError> {
        if self.pending_utf8_continuations != 0 {
            return Err(SourceFactsError::CorruptSource("truncated UTF-8 scalar"));
        }
        if self.pending_carriage_return {
            self.logical_line_breaks = self
                .logical_line_breaks
                .checked_add(1)
                .ok_or(SourceFactsError::CounterExhausted)?;
            self.pending_carriage_return = false;
        }
        if self.byte_offset != self.scan_byte_len || self.utf16_offset != self.scan_utf16_len {
            return Err(SourceFactsError::CorruptSource(
                "scanned dimensions disagree with source version",
            ));
        }
        Ok(())
    }

    fn completion(&self) -> Result<SourceFactsCompletion, SourceFactsError> {
        Ok(SourceFactsCompletion {
            scan_id: self.scan_id,
            source: self.source,
            checkpoint_spacing_utf16: self.checkpoint_spacing_utf16,
            checkpoint_sequence_digest: self.checkpoint_sequence_digest(),
            fingerprint: SourceContentFingerprint {
                algorithm: SOURCE_CONTENT_FINGERPRINT_ALGORITHM,
                byte_len: self.scan_byte_len,
                utf16_len: self.scan_utf16_len,
                rolling_hash: self.hash,
            },
            logical_line_breaks: self.logical_line_breaks,
            checkpoint_count: self.checkpoint_count,
            page_count: self.page_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::persistent_sequence_codec::{
        add_mod, combine_sequence_commitments, decode_source_fact_sequence_leaf,
        encode_source_fact_sequence_leaf, encode_source_fact_sequence_leaf_unchecked_for_test,
        leaf_sequence_commitment, mul_mod, DecodedSourceFactSequenceLeaf,
        SourceFactsSequenceCommitment, SourceFactsSequenceSpec, COMMITMENT_BASES,
        COMMITMENT_MODULUS,
    };
    use super::*;
    use crate::measured_sequence::{
        begin_measured_sequence_seal, splice_measured_sequence_atomic,
        CommittedMeasuredSequenceRoot, MeasuredSequenceBuildRoot, ResumableMeasuredSequenceBuilder,
        ResumableSequenceProgress, SequenceMutationReceipt,
    };
    use crate::source::SourceStore;
    use crate::storage::{ArenaBuildSession, ArenaLimits, CandidateBuild, PageArena};

    fn parser_profile() -> ParserProfileId {
        ParserProfileId::new(7).expect("test parser profile should be nonzero")
    }

    fn decode_persistent_leaf<'payload>(
        payload: &'payload [u8],
    ) -> Result<Option<DecodedSourceFactSequenceLeaf<'payload>>, SourceFactsAssemblyError> {
        decode_source_fact_sequence_leaf(payload, &mut SequenceSpecInspection::default())
    }

    fn root_builder(
        lease: SourceSnapshotLease,
        profile: SourceFactsScanProfile,
    ) -> Result<SourceFactsRootBuilder, SourceFactsAssemblyError> {
        SourceFactsRootBuilder::new(
            lease,
            profile,
            parser_profile(),
            SourceFactsRootLimits::default(),
        )
    }

    fn scan_lease(
        lease: &SourceSnapshotLease,
        profile: SourceFactsScanProfile,
        source_fuel: usize,
        checkpoint_fuel: usize,
    ) -> (Vec<SourceFactCheckpointPage>, SourceFactsCompletion) {
        let expected = lease.version();
        let mut scanner = SourceFactsScanner::with_profile(lease.duplicate(), profile)
            .expect("scanner should build");
        let mut pages = Vec::new();
        loop {
            match scanner
                .poll(source_fuel, checkpoint_fuel)
                .expect("scan should succeed")
            {
                SourceFactsPoll::Pending(work) => {
                    assert!(work.source_bytes_examined() <= source_fuel);
                    assert!(work.source_bytes_buffered() <= source_fuel);
                    assert_eq!(
                        work.cursor_copy_bytes_upper_bound(),
                        work.cursor_refills() * SOURCE_CURSOR_WINDOW_BYTES
                    );
                }
                SourceFactsPoll::Page { page, work } => {
                    assert_eq!(page.source(), expected);
                    assert!(page.checkpoints().len() <= checkpoint_fuel);
                    assert!(work.source_bytes_examined() <= source_fuel);
                    assert!(work.source_bytes_buffered() <= source_fuel);
                    pages.push(page);
                }
                SourceFactsPoll::Complete { completion, work } => {
                    assert_eq!(completion.source(), expected);
                    assert!(work.source_bytes_examined() <= source_fuel);
                    assert!(work.source_bytes_buffered() <= source_fuel);
                    return (pages, completion);
                }
                SourceFactsPoll::Cancelled => panic!("scan unexpectedly cancelled"),
            }
        }
    }

    fn scan(
        source: &str,
        checkpoint_spacing_utf16: usize,
        source_fuel: usize,
        checkpoint_fuel: usize,
    ) -> (Vec<SourceFactCheckpointPage>, SourceFactsCompletion) {
        let store = SourceStore::new(source).expect("source should build");
        let profile =
            SourceFactsScanProfile::new(checkpoint_spacing_utf16).expect("profile should build");
        scan_lease(&store.snapshot(), profile, source_fuel, checkpoint_fuel)
    }

    fn certify(
        lease: SourceSnapshotLease,
        profile: SourceFactsScanProfile,
        pages: Vec<SourceFactCheckpointPage>,
        completion: SourceFactsCompletion,
    ) -> Result<CertifiedSource, SourceFactsAssemblyError> {
        let mut builder = root_builder(lease, profile)?;
        for page in pages {
            builder.push_page(page)?;
        }
        builder.certify(completion)
    }

    fn digest_for_pages(
        pages: &mut [SourceFactCheckpointPage],
        spacing: u64,
    ) -> SourceFactSequenceDigest {
        let mut hasher = checkpoint_sequence_hasher(spacing);
        for page in pages {
            for checkpoint in page.checkpoints.iter().copied() {
                append_checkpoint_to_hasher(&mut hasher, checkpoint);
            }
            page.sequence_digest = checkpoint_sequence_digest(&hasher);
        }
        checkpoint_sequence_digest(&hasher)
    }

    fn forged_persistent_leaf_payload(
        page: &SourceFactCanonicalPage,
        profile: SourceFactsScanProfile,
        mutate: impl FnOnce(&mut [SourceFactRelativeCheckpoint]),
    ) -> Vec<u8> {
        let mut checkpoints = page.checkpoints().to_vec();
        mutate(&mut checkpoints);
        let summary = checkpoints
            .last()
            .copied()
            .expect("test leaf should remain nonempty")
            .summary;
        let content_digest =
            source_fact_page_digest(profile.checkpoint_spacing_utf16, summary, &checkpoints);
        let forged = SourceFactCanonicalPage {
            content_digest,
            summary,
            checkpoints: checkpoints.into_boxed_slice(),
        };
        let mut payload = [0_u8; ARENA_PAGE_BYTES];
        let encoded =
            encode_source_fact_sequence_leaf_unchecked_for_test(&forged, profile, &mut payload)
                .expect("forged test payload should remain wire-encodable");
        payload[..encoded].to_vec()
    }

    fn summary_for(source: &str) -> SourceFactSegmentSummary {
        let bytes = source.as_bytes();
        let mut rolling_hash = SourceContentHash128::default();
        let mut logical_line_breaks = 0_u64;
        let mut pending_carriage_return = false;
        for byte in bytes.iter().copied() {
            let followed_carriage_return = pending_carriage_return;
            if followed_carriage_return {
                logical_line_breaks += 1;
                pending_carriage_return = false;
            }
            rolling_hash.append_byte(byte);
            match byte {
                b'\r' => pending_carriage_return = true,
                b'\n' if !followed_carriage_return => logical_line_breaks += 1,
                _ => {}
            }
        }
        if pending_carriage_return {
            logical_line_breaks += 1;
        }
        SourceFactSegmentSummary {
            byte_len: bytes.len() as u64,
            utf16_len: source.encode_utf16().count() as u64,
            logical_line_breaks,
            rolling_hash,
            starts_with_line_feed: bytes.first() == Some(&b'\n'),
            ends_with_carriage_return: bytes.last() == Some(&b'\r'),
        }
    }

    fn certified(source: &str, spacing: usize) -> CertifiedSource {
        let store = SourceStore::new(source).expect("source should build");
        let lease = store.snapshot();
        let profile = SourceFactsScanProfile::new(spacing).expect("profile should build");
        let (pages, completion) = scan_lease(&lease, profile, 17, 3);
        certify(lease, profile, pages, completion).expect("source should certify")
    }

    fn certified_range(
        store: &SourceStore,
        range: std::ops::Range<usize>,
        spacing: usize,
    ) -> CertifiedSource {
        let lease = store.snapshot();
        let profile = SourceFactsScanProfile::new(spacing).expect("profile should build");
        let mut scanner =
            SourceFactsScanner::with_profile_range(lease.duplicate(), profile, range.clone())
                .expect("range scanner");
        let mut builder = SourceFactsRootBuilder::new_range(
            lease,
            range,
            profile,
            parser_profile(),
            SourceFactsRootLimits::default(),
        )
        .expect("range builder");
        loop {
            match scanner.poll(17, 3).expect("range scan") {
                SourceFactsPoll::Pending(_) => {}
                SourceFactsPoll::Page { page, .. } => {
                    builder.push_page(page).expect("range page");
                }
                SourceFactsPoll::Complete { completion, .. } => {
                    return builder
                        .finish_segment(completion)
                        .expect("exact range certification");
                }
                SourceFactsPoll::Cancelled => panic!("range scan unexpectedly cancelled"),
            }
        }
    }

    fn promote_persistent_source_facts(
        arena: &mut PageArena,
        certified: CertifiedSource,
    ) -> PersistentSourceFactsRoot {
        let mut build = PersistentSourceFactsBuild::new(certified);
        loop {
            match build.poll(arena).expect("persistent promotion") {
                PersistentSourceFactsBuildPoll::Pending => {}
                PersistentSourceFactsBuildPoll::Complete(output) => {
                    let PersistentSourceFactsBuildOutput { certified, root } = *output;
                    drop(certified);
                    return root;
                }
            }
        }
    }

    fn drain_arena(arena: &mut PageArena) {
        while arena.metrics().pending_reclaims != 0 || arena.metrics().pending_build_aborts != 0 {
            let receipt = arena.poll_reclaim(1);
            assert!(receipt.transitions <= 1);
        }
    }

    fn build_source_fact_sequence_root(
        session: &mut ArenaBuildSession<'_>,
        facts: &SourceFactsRoot,
        receipt: &mut SequenceMutationReceipt,
    ) -> MeasuredSequenceBuildRoot<SourceFactsSequenceSpec> {
        assert!(facts.page_count() > 0);
        let mut builder =
            ResumableMeasuredSequenceBuilder::<SourceFactsSequenceSpec>::try_new(session, receipt)
                .expect("sequence builder");
        for page in facts.pages() {
            let mut payload = [0_u8; ARENA_PAGE_BYTES];
            let encoded =
                encode_source_fact_sequence_leaf(page.canonical(), facts.profile(), &mut payload)
                    .expect("encode source-fact leaf");
            let leaf = session
                .allocate(&payload[..encoded], &[])
                .expect("allocate source-fact leaf");
            builder
                .begin_push(session, leaf, receipt)
                .expect("begin source-fact leaf");
            while builder
                .poll_push(session, receipt)
                .expect("poll source-fact leaf")
                == ResumableSequenceProgress::Pending
            {}
        }
        builder
            .begin_finish(session, receipt)
            .expect("begin source-fact finish");
        while builder
            .poll_finish(session, receipt)
            .expect("poll source-fact finish")
            == ResumableSequenceProgress::Pending
        {}
        builder.take_root(session).expect("take source-fact root")
    }

    fn seal_source_fact_sequence(
        arena: &mut PageArena,
        build: CandidateBuild,
        root: MeasuredSequenceBuildRoot<SourceFactsSequenceSpec>,
    ) -> CommittedMeasuredSequenceRoot<SourceFactsSequenceSpec> {
        let mut seal = match begin_measured_sequence_seal(arena, build, root) {
            Ok(seal) => seal,
            Err(_) => panic!("begin source-fact seal"),
        };
        loop {
            let poll = seal.poll(arena, 1).expect("poll source-fact seal");
            if let Some(root) = poll.root {
                return root;
            }
        }
    }

    fn build_committed_source_fact_sequence(
        arena: &mut PageArena,
        facts: &SourceFactsRoot,
    ) -> CommittedMeasuredSequenceRoot<SourceFactsSequenceSpec> {
        let mut receipt = SequenceMutationReceipt::default();
        let (build, root) = {
            let mut session = arena.begin_build().expect("source-fact build");
            let root = build_source_fact_sequence_root(&mut session, facts, &mut receipt);
            (session.suspend().expect("suspend source-fact build"), root)
        };
        seal_source_fact_sequence(arena, build, root)
    }
    fn reference_mod_pow(mut base: u64, mut exponent: u64) -> u64 {
        let mut value = 1_u64;
        while exponent != 0 {
            if exponent & 1 == 1 {
                value = u64::try_from(
                    (u128::from(value) * u128::from(base)) % u128::from(COMMITMENT_MODULUS),
                )
                .expect("reduced product");
            }
            base = u64::try_from(
                (u128::from(base) * u128::from(base)) % u128::from(COMMITMENT_MODULUS),
            )
            .expect("reduced square");
            exponent >>= 1;
        }
        value
    }

    fn reference_ordered_commitment(
        leaves: &[SourceFactsSequenceCommitment],
    ) -> SourceFactsSequenceCommitment {
        let mut values = [0_u64; COMMITMENT_BASES.len()];
        let mut powers = [1_u64; COMMITMENT_BASES.len()];
        let leaf_count = u64::try_from(leaves.len()).expect("test leaf count");
        for lane in 0..COMMITMENT_BASES.len() {
            powers[lane] = reference_mod_pow(COMMITMENT_BASES[lane], leaf_count);
            for (ordinal, leaf) in leaves.iter().enumerate() {
                assert_eq!(leaf.powers, COMMITMENT_BASES);
                let remaining = leaves.len() - ordinal - 1;
                let positional_power = reference_mod_pow(
                    COMMITMENT_BASES[lane],
                    u64::try_from(remaining).expect("test exponent"),
                );
                let term = u64::try_from(
                    (u128::from(leaf.values[lane]) * u128::from(positional_power))
                        % u128::from(COMMITMENT_MODULUS),
                )
                .expect("reduced reference term");
                values[lane] = u64::try_from(
                    (u128::from(values[lane]) + u128::from(term)) % u128::from(COMMITMENT_MODULUS),
                )
                .expect("reduced reference sum");
            }
        }
        SourceFactsSequenceCommitment { values, powers }
    }

    #[test]
    fn ordered_commitment_field_and_fixed_bases_are_regression_locked() {
        let boundary_values = [
            0,
            1,
            2,
            COMMITMENT_MODULUS / 2,
            COMMITMENT_MODULUS - 2,
            COMMITMENT_MODULUS - 1,
            u64::MAX,
        ];
        for left in boundary_values {
            for right in boundary_values {
                let expected_add = u64::try_from(
                    (u128::from(left) + u128::from(right)) % u128::from(COMMITMENT_MODULUS),
                )
                .expect("reduced sum");
                let expected_mul = u64::try_from(
                    (u128::from(left) * u128::from(right)) % u128::from(COMMITMENT_MODULUS),
                )
                .expect("reduced product");
                assert_eq!(add_mod(left, right), expected_add);
                assert_eq!(mul_mod(left, right), expected_mul);
            }
        }

        let prime_divisors = [2_u64, 3, 5, 7, 11, 13, 31, 41, 61, 151, 331, 1_321];
        let expected_orders = [
            (COMMITMENT_MODULUS - 1) / 2,
            COMMITMENT_MODULUS - 1,
            (COMMITMENT_MODULUS - 1) / 2,
            (COMMITMENT_MODULUS - 1) / 2,
            COMMITMENT_MODULUS - 1,
        ];
        for (base, expected_order) in COMMITMENT_BASES.into_iter().zip(expected_orders) {
            let mut order = COMMITMENT_MODULUS - 1;
            for divisor in prime_divisors {
                while order.is_multiple_of(divisor) && reference_mod_pow(base, order / divisor) == 1
                {
                    order /= divisor;
                }
            }
            assert_eq!(order, expected_order);
        }
    }

    #[test]
    fn ordered_commitment_is_associative_shape_independent_and_order_sensitive() {
        let [alpha, beta, gamma, delta] =
            [&b"alpha"[..], &b"beta"[..], &b"gamma"[..], &b"delta"[..]]
                .map(leaf_sequence_commitment);
        let left_grouped = combine_sequence_commitments(
            combine_sequence_commitments(combine_sequence_commitments(alpha, beta), gamma),
            delta,
        );
        let balanced = combine_sequence_commitments(
            combine_sequence_commitments(alpha, beta),
            combine_sequence_commitments(gamma, delta),
        );
        let right_grouped = combine_sequence_commitments(
            alpha,
            combine_sequence_commitments(beta, combine_sequence_commitments(gamma, delta)),
        );
        assert_eq!(left_grouped, balanced);
        assert_eq!(balanced, right_grouped);
        assert_eq!(
            balanced,
            reference_ordered_commitment(&[alpha, beta, gamma, delta])
        );
        assert_ne!(
            balanced,
            combine_sequence_commitments(
                combine_sequence_commitments(alpha, gamma),
                combine_sequence_commitments(beta, delta),
            )
        );

        let vector = leaf_sequence_commitment(b"flark-source-facts-canonical-vector");
        assert_eq!(
            vector.values,
            [
                1_357_002_948_561_573_264,
                1_228_993_707_702_122_165,
                1_346_893_154_524_133_468,
                111_236_106_490_193_104,
                1_238_815_052_186_562_531,
            ]
        );
        assert_eq!(vector.powers, COMMITMENT_BASES);
    }

    #[test]
    fn clean_promotion_anchors_tree_commitment_to_certified_page_order() {
        let certified = certified(&"a\r\n🌍x".repeat(600), 2);
        assert!(certified.facts().page_count() > 8);
        let mut leaf_commitments = Vec::new();
        for page in certified.facts().pages() {
            let mut payload = [0_u8; ARENA_PAGE_BYTES];
            let encoded = encode_source_fact_sequence_leaf(
                page.canonical(),
                certified.facts().profile(),
                &mut payload,
            )
            .expect("canonical leaf");
            leaf_commitments.push(leaf_sequence_commitment(&payload[..encoded]));
        }
        let expected = reference_ordered_commitment(&leaf_commitments);

        let mut arena = PageArena::new(ArenaLimits {
            max_slots: 8_192,
            max_live_payload_bytes: 32 * 1024 * 1024,
            max_children_per_node: 8,
        })
        .expect("arena");
        let root = promote_persistent_source_facts(&mut arena, certified);
        assert_eq!(root.expected_commitment, expected);
        let mut inspection = SequenceInspectionReceipt::default();
        let actual = root
            .tree
            .as_ref()
            .expect("nonempty tree")
            .as_ref()
            .summary(&arena, &mut inspection)
            .expect("tree summary")
            .expect("root measure")
            .summary()
            .commitment;
        assert_eq!(actual, expected);
        assert!(root.release(&mut arena).is_ok());
        drain_arena(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn multiple_splices_preserve_the_independently_composed_ordered_commitment() {
        let source = "abcd".repeat(600);
        let spacing = 2;
        let base_certified = certified(&source, spacing);
        assert!(base_certified.facts().page_count() > 10);
        let mut boundaries = Vec::with_capacity(base_certified.facts().pages().len() + 1);
        boundaries.push(0_usize);
        for page in base_certified.facts().pages() {
            boundaries.push(
                boundaries.last().copied().expect("initial boundary")
                    + usize::try_from(page.canonical().summary().byte_len())
                        .expect("page byte length"),
            );
        }

        let mut arena = PageArena::new(ArenaLimits {
            max_slots: 16_384,
            max_live_payload_bytes: 64 * 1024 * 1024,
            max_children_per_node: 8,
        })
        .expect("arena");
        let base = promote_persistent_source_facts(&mut arena, base_certified);
        let expected = base.expected_commitment;

        let first_target = SourceStore::new(&source).expect("first target");
        let first_range = 2_u64..5;
        let first_replacement = promote_persistent_source_facts(
            &mut arena,
            certified_range(
                &first_target,
                boundaries[first_range.start as usize]..boundaries[first_range.end as usize],
                spacing,
            ),
        );
        let first = splice_persistent_source_facts_atomic(
            &mut arena,
            &base,
            &first_replacement,
            first_range,
            first_target.version(),
        )
        .expect("first persistent splice");
        assert_eq!(first.expected_commitment, expected);
        assert!(base.release(&mut arena).is_ok());
        assert!(first_replacement.release(&mut arena).is_ok());

        let second_target = SourceStore::new(&source).expect("second target");
        let second_range = 7_u64..9;
        let second_replacement = promote_persistent_source_facts(
            &mut arena,
            certified_range(
                &second_target,
                boundaries[second_range.start as usize]..boundaries[second_range.end as usize],
                spacing,
            ),
        );
        let second = splice_persistent_source_facts_atomic(
            &mut arena,
            &first,
            &second_replacement,
            second_range,
            second_target.version(),
        )
        .expect("second persistent splice");
        assert_eq!(second.expected_commitment, expected);
        assert!(first.release(&mut arena).is_ok());
        assert!(second_replacement.release(&mut arena).is_ok());
        assert!(second.release(&mut arena).is_ok());
        drain_arena(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn changed_content_splice_matches_a_separately_clean_promoted_target_commitment() {
        let source = "abcd".repeat(600);
        let spacing = 2;
        let base_certified = certified(&source, spacing);
        let mut boundaries = Vec::with_capacity(base_certified.facts().pages().len() + 1);
        boundaries.push(0_usize);
        for page in base_certified.facts().pages() {
            boundaries.push(
                boundaries.last().copied().expect("initial boundary")
                    + usize::try_from(page.canonical().summary().byte_len())
                        .expect("page byte length"),
            );
        }
        assert!(boundaries.len() > 7);

        let mut target_text = source.clone();
        let edit = boundaries[3] + 17;
        target_text.replace_range(edit..edit + 1, "Z");
        let target_store = SourceStore::new(&target_text).expect("target source");
        let replacement_range = 2_u64..6;
        let replacement_bytes = boundaries[replacement_range.start as usize]
            ..boundaries[replacement_range.end as usize];

        let mut arena = PageArena::new(ArenaLimits {
            max_slots: 16_384,
            max_live_payload_bytes: 64 * 1024 * 1024,
            max_children_per_node: 8,
        })
        .expect("arena");
        let base = promote_persistent_source_facts(&mut arena, base_certified);
        let base_authority = base.authority_snapshot();
        let replacement = promote_persistent_source_facts(
            &mut arena,
            certified_range(&target_store, replacement_bytes, spacing),
        );
        let output = splice_persistent_source_facts_atomic_with_receipt(
            &mut arena,
            &base,
            &replacement,
            replacement_range,
            target_store.version(),
        )
        .expect("changed-content splice");
        let (updated, splice_work) = output.into_parts();
        let clean_target =
            promote_persistent_source_facts(&mut arena, certified(&target_text, spacing));

        assert!(base.authority_snapshot() == base_authority);
        assert!(updated.authority_snapshot() != base_authority);
        assert_eq!(
            updated.work,
            replacement
                .work
                .checked_add(splice_work)
                .expect("cumulative splice work")
        );
        assert_ne!(base.expected_commitment, updated.expected_commitment);
        assert_eq!(
            updated.expected_commitment,
            clean_target.expected_commitment
        );
        let mut updated_inspection = SequenceInspectionReceipt::default();
        let updated_actual = updated
            .tree
            .as_ref()
            .expect("updated tree")
            .as_ref()
            .summary(&arena, &mut updated_inspection)
            .expect("updated summary")
            .expect("updated root")
            .summary()
            .commitment;
        assert_eq!(updated_actual, clean_target.expected_commitment);

        assert!(base.release(&mut arena).is_ok());
        assert!(replacement.release(&mut arena).is_ok());
        assert!(updated.release(&mut arena).is_ok());
        assert!(clean_target.release(&mut arena).is_ok());
        drain_arena(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }
    #[test]
    fn segment_summaries_compose_exactly_across_every_scalar_cut() {
        let source = "\nalpha\r\nbeta\rgamma\n🌍delta\r\n";
        let expected = summary_for(source);
        for cut in (0..=source.len()).filter(|cut| source.is_char_boundary(*cut)) {
            let left = summary_for(&source[..cut]);
            let right = summary_for(&source[cut..]);
            assert_eq!(
                left.checked_followed_by(right),
                Some(expected),
                "composition diverged at byte cut {cut}"
            );
        }
    }

    #[test]
    fn range_scanner_reports_segment_local_facts_under_full_source_authority() {
        let source = "prefix|a\r\n🌍b\rc\n|suffix";
        let start = source.find('a').expect("crop start");
        let end = source.rfind('|').expect("crop end");
        let expected_text = &source[start..end];
        let expected = summary_for(expected_text);
        let store = SourceStore::new(source).expect("source");
        let version = store.version();
        let profile = SourceFactsScanProfile::new(2).expect("profile");
        let mut scanner =
            SourceFactsScanner::with_profile_range(store.snapshot(), profile, start..end)
                .expect("range scanner");
        let mut checkpoints = Vec::new();
        let completion = loop {
            match scanner.poll(3, 2).expect("range poll") {
                SourceFactsPoll::Pending(_) => {}
                SourceFactsPoll::Page { page, .. } => {
                    assert_eq!(page.source(), version);
                    checkpoints.extend_from_slice(page.checkpoints());
                }
                SourceFactsPoll::Complete { completion, .. } => break completion,
                SourceFactsPoll::Cancelled => panic!("range scan cancelled"),
            }
        };
        assert_eq!(completion.source(), version);
        assert_eq!(completion.fingerprint().byte_len(), expected.byte_len());
        assert_eq!(completion.fingerprint().utf16_len(), expected.utf16_len());
        assert_eq!(
            completion.fingerprint().rolling_hash(),
            expected.rolling_hash()
        );
        assert_eq!(
            completion.logical_line_breaks(),
            expected.logical_line_breaks()
        );
        let terminal = checkpoints.last().expect("terminal checkpoint");
        assert_eq!(terminal.byte_offset(), expected.byte_len());
        assert_eq!(terminal.utf16_offset(), expected.utf16_len());
        assert_eq!(terminal.rolling_hash(), expected.rolling_hash());
    }

    #[test]
    fn segment_summary_composition_is_associative_and_folds_split_crlf() {
        let left = summary_for("a\r");
        let middle = summary_for("\n🌍\r");
        let right = summary_for("\nb");
        let expected = summary_for("a\r\n🌍\r\nb");
        assert_eq!(
            left.checked_followed_by(middle)
                .and_then(|summary| summary.checked_followed_by(right)),
            Some(expected)
        );
        assert_eq!(
            middle
                .checked_followed_by(right)
                .and_then(|summary| left.checked_followed_by(summary)),
            Some(expected)
        );
        assert_eq!(expected.logical_line_breaks(), 2);
    }

    #[test]
    fn segment_summary_composition_rejects_malformed_or_overflowed_metrics() {
        let malformed_empty = SourceFactSegmentSummary {
            starts_with_line_feed: true,
            ..SourceFactSegmentSummary::default()
        };
        assert_eq!(malformed_empty.checked_followed_by(summary_for("a")), None);

        let oversized = SourceFactSegmentSummary {
            byte_len: u64::MAX,
            utf16_len: 1,
            rolling_hash: summary_for("a").rolling_hash(),
            ..SourceFactSegmentSummary::default()
        };
        assert_eq!(oversized.checked_followed_by(summary_for("b")), None);
    }

    #[test]
    fn canonical_eof_carriage_return_composes_with_an_appended_line_feed() {
        let certified = certified("a\r", 2);
        let terminal = certified
            .facts()
            .pages()
            .next_back()
            .expect("nonempty source should have a terminal page")
            .canonical()
            .summary();
        assert!(terminal.ends_with_carriage_return());
        assert_eq!(
            terminal.checked_followed_by(summary_for("\n")),
            Some(summary_for("a\r\n"))
        );
    }

    #[test]
    fn canonical_page_content_is_independent_of_document_prefix() {
        let suffix = "z".repeat(128);
        let first = certified(&("a".repeat(128) + &suffix), 2);
        let shifted = certified(&("b".repeat(256) + &suffix), 2);
        let first_page = first
            .facts()
            .pages()
            .nth(1)
            .expect("suffix should occupy the second canonical page");
        let shifted_page = shifted
            .facts()
            .pages()
            .nth(2)
            .expect("suffix should occupy the third canonical page");

        assert_eq!(
            first_page.canonical().summary(),
            shifted_page.canonical().summary()
        );
        assert_eq!(
            first_page.canonical().checkpoints(),
            shifted_page.canonical().checkpoints()
        );
        assert_eq!(first_page.content_digest(), shifted_page.content_digest());
        assert_ne!(first_page.checkpoints(), shifted_page.checkpoints());
    }

    #[test]
    fn canonical_page_summaries_reconstruct_the_clean_completion() {
        let source = "a\r\n🌍b\rc\n".repeat(500);
        let certified = certified(&source, 2);
        let composed = certified
            .facts()
            .pages()
            .try_fold(SourceFactSegmentSummary::default(), |prefix, page| {
                prefix.checked_followed_by(page.canonical().summary())
            })
            .expect("page summaries should compose without overflow");
        let fingerprint = certified.facts().fingerprint();
        assert_eq!(composed.byte_len(), fingerprint.byte_len());
        assert_eq!(composed.utf16_len(), fingerprint.utf16_len());
        assert_eq!(
            composed.logical_line_breaks(),
            certified.facts().logical_line_breaks()
        );
        assert_eq!(composed.rolling_hash(), fingerprint.rolling_hash());
    }

    #[test]
    fn canonical_pages_form_one_typed_arena_sequence_without_absolute_coordinates() {
        // The 64th spacing-2 checkpoint is delayed until after the CRLF, so
        // this also exercises the permitted one-unit safe-boundary overshoot.
        let source = format!("{}\r\n{}", "x".repeat(127), "y🌍\n".repeat(200));
        let certified = certified(&source, 2);
        let facts = certified.facts();
        assert!(facts.page_count() >= 2);
        let limits = ArenaLimits {
            max_slots: 4_096,
            max_live_payload_bytes: 16 * 1024 * 1024,
            max_children_per_node: 4,
        };
        let mut arena = PageArena::new(limits).expect("arena");
        let mut receipt = SequenceMutationReceipt::default();
        let mut session = arena.begin_build().expect("build");
        let mut builder = ResumableMeasuredSequenceBuilder::<SourceFactsSequenceSpec>::try_new(
            &mut session,
            &mut receipt,
        )
        .expect("sequence builder");

        for page in facts.pages() {
            let mut payload = [0_u8; ARENA_PAGE_BYTES];
            let encoded =
                encode_source_fact_sequence_leaf(page.canonical(), facts.profile(), &mut payload)
                    .expect("encode canonical page");
            assert!(encoded <= ARENA_PAGE_BYTES);
            let decoded = decode_persistent_leaf(&payload[..encoded])
                .expect("decode canonical page")
                .expect("leaf payload");
            assert_eq!(decoded.content_digest, page.content_digest());
            assert_eq!(
                decoded.checkpoint_count(),
                page.canonical().checkpoints().len()
            );
            for (ordinal, expected) in page.canonical().checkpoints().iter().enumerate() {
                assert_eq!(decoded.checkpoint(ordinal), Ok(Some(*expected)));
            }

            let leaf = session
                .allocate(&payload[..encoded], &[])
                .expect("allocate canonical leaf");
            builder
                .begin_push(&session, leaf, &mut receipt)
                .expect("begin leaf push");
            while builder
                .poll_push(&mut session, &mut receipt)
                .expect("poll leaf push")
                == ResumableSequenceProgress::Pending
            {}
        }
        builder
            .begin_finish(&session, &mut receipt)
            .expect("begin finish");
        while builder
            .poll_finish(&mut session, &mut receipt)
            .expect("poll finish")
            == ResumableSequenceProgress::Pending
        {}
        let root = builder.take_root(&session).expect("take root");
        let build = session.suspend().expect("suspend root");
        let mut seal = match begin_measured_sequence_seal(&mut arena, build, root) {
            Ok(seal) => seal,
            Err(_) => panic!("begin seal"),
        };
        let root = loop {
            let poll = seal.poll(&mut arena, 1).expect("poll seal");
            if let Some(root) = poll.root {
                break root;
            }
        };

        let sequence = root.as_ref();
        let mut summary_inspection = SequenceInspectionReceipt::default();
        let measure = sequence
            .summary(&arena, &mut summary_inspection)
            .expect("sequence summary")
            .expect("nonempty root");
        assert_eq!(summary_inspection.node_headers_decoded, 3);
        assert_eq!(summary_inspection.summary_combinations, 1);
        assert!(
            summary_inspection.spec.payload_bytes_inspected
                <= 3 * u64::try_from(ARENA_PAGE_BYTES).expect("page size fits u64")
        );
        assert!(
            summary_inspection.spec.spec_items_hashed
                <= 2 * SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX as u64
        );
        assert_eq!(measure.leaves(), facts.page_count());
        assert_eq!(measure.summary().checkpoint_count, facts.checkpoint_count());
        assert_eq!(measure.summary().segment, facts.summary());
        assert_eq!(
            measure.summary().segment.logical_line_breaks(),
            source.matches('\n').count() as u64
        );

        let mut expected_prefix = SourceFactSegmentSummary::default();
        for (ordinal, page) in facts.pages().enumerate() {
            let mut locate_inspection = SequenceInspectionReceipt::default();
            let located = sequence
                .locate_leaf_with_prefix(&arena, ordinal as u64, &mut locate_inspection)
                .expect("locate page")
                .expect("present page");
            assert!(locate_inspection.node_headers_decoded >= 2);
            assert!(locate_inspection.spec.payload_bytes_inspected > 0);
            assert!(locate_inspection.spec.spec_items_hashed > 0);
            assert_eq!(
                located.prefix.map(|summary| summary.segment),
                (ordinal != 0).then_some(expected_prefix)
            );
            let decoded = decode_persistent_leaf(arena.payload(located.id).expect("leaf payload"))
                .expect("decode located page")
                .expect("located leaf");
            assert_eq!(decoded.content_digest, page.content_digest());
            let mut materialize_inspection = SequenceInspectionReceipt::default();
            let absolute = materialize_source_facts_absolute_page(
                sequence,
                &arena,
                ordinal as u64,
                &mut materialize_inspection,
            )
            .expect("derive absolute page")
            .expect("absolute page");
            assert_eq!(
                materialize_inspection.node_headers_decoded,
                locate_inspection.node_headers_decoded
            );
            assert_eq!(
                materialize_inspection.spec.payload_bytes_inspected
                    - locate_inspection.spec.payload_bytes_inspected,
                arena.payload(located.id).expect("leaf payload").len() as u64
            );
            assert_eq!(
                materialize_inspection.spec.spec_items_hashed
                    - locate_inspection.spec.spec_items_hashed,
                decoded.checkpoint_count() as u64
            );
            assert_eq!(absolute.content_digest(), page.content_digest());
            assert_eq!(absolute.checkpoints(), page.checkpoints());
            expected_prefix = expected_prefix
                .checked_followed_by(page.canonical().summary())
                .expect("compose expected prefix");
        }

        assert!(root.release(&mut arena).is_ok());
        drain_arena(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn arbitrary_length_crop_splice_reuses_exact_suffix_with_semantic_clean_equality() {
        let old_text = (0..3_000)
            .map(|ordinal| format!("{ordinal:04x}|"))
            .collect::<String>();
        let edit = 1_234..1_235;
        let replacement = "WXYZ";
        let mut target_text = old_text.clone();
        target_text.replace_range(edit.clone(), replacement);

        let spacing = 8;
        let old_certified = certified(&old_text, spacing);
        let target_clean = certified(&target_text, spacing);
        let old_facts = old_certified.facts();
        let target_facts = target_clean.facts();
        assert!(old_facts.page_count() > 8);

        // The current full-clean scanner is phase-locked to document-global
        // UTF-16 offsets. This length-changing edit shifts every later page,
        // so a second clean scan has no identical canonical suffix to adopt.
        let old_digests = old_facts
            .pages()
            .map(|page| page.content_digest())
            .collect::<Vec<_>>();
        let target_digests = target_facts
            .pages()
            .map(|page| page.content_digest())
            .collect::<Vec<_>>();
        let common_clean_suffix = old_digests
            .iter()
            .rev()
            .zip(target_digests.iter().rev())
            .take_while(|(old, target)| old == target)
            .count();
        assert_eq!(common_clean_suffix, 0);

        let mut old_boundaries = Vec::with_capacity(old_facts.pages().len() + 1);
        old_boundaries.push(0_usize);
        for page in old_facts.pages() {
            let next = old_boundaries
                .last()
                .copied()
                .expect("initial boundary")
                .checked_add(page.canonical().summary().byte_len() as usize)
                .expect("page boundary");
            old_boundaries.push(next);
        }
        assert_eq!(old_boundaries.last().copied(), Some(old_text.len()));
        let restart_page = old_boundaries
            .windows(2)
            .position(|bounds| bounds[0] <= edit.start && edit.start < bounds[1])
            .expect("edit-containing page");
        let suffix_page = old_boundaries
            .iter()
            .position(|&boundary| boundary >= edit.end)
            .expect("suffix page boundary");
        assert!(restart_page < suffix_page);

        let mut source = SourceStore::new(&old_text).expect("lineage source");
        let previous = source.version();
        let prepared = source
            .prepare_edit(previous, edit.clone(), replacement)
            .expect("prepare lineage edit");
        let (_edit_receipt, retired, lineage) = source
            .commit_prepared_edit(prepared)
            .expect("commit lineage edit")
            .into_parts_with_lineage();
        let current = source.version();
        let target_restart = lineage
            .map_byte_boundary(
                previous,
                current,
                old_boundaries[restart_page],
                crate::source::SourceBoundaryAffinity::Before,
            )
            .expect("map restart boundary");
        let target_suffix = lineage
            .map_byte_boundary(
                previous,
                current,
                old_boundaries[suffix_page],
                crate::source::SourceBoundaryAffinity::After,
            )
            .expect("map suffix boundary");
        assert!(target_restart < target_suffix);
        let replacement_certified = certified(&target_text[target_restart..target_suffix], spacing);
        let replacement_facts = replacement_certified.facts();

        let limits = ArenaLimits {
            max_slots: 8_192,
            max_live_payload_bytes: 32 * 1024 * 1024,
            max_children_per_node: 4,
        };
        let mut arena = PageArena::new(limits).expect("arena");
        let base = build_committed_source_fact_sequence(&mut arena, old_facts);
        let base_sequence = base.as_ref();
        let base_ids = (0..old_facts.page_count())
            .map(|ordinal| {
                let mut inspection = SequenceInspectionReceipt::default();
                base_sequence
                    .locate_leaf_with_prefix(&arena, ordinal, &mut inspection)
                    .expect("locate base page")
                    .expect("base page")
                    .id
            })
            .collect::<Vec<_>>();

        let mut receipt = SequenceMutationReceipt::default();
        let (build, updated_root) = {
            let mut session = arena.begin_build().expect("incremental build");
            let replacement_root =
                build_source_fact_sequence_root(&mut session, replacement_facts, &mut receipt);
            let root = splice_measured_sequence_atomic::<SourceFactsSequenceSpec>(
                &mut session,
                &base,
                restart_page as u64..suffix_page as u64,
                Some(replacement_root),
                &mut receipt,
            )
            .expect("source-fact splice")
            .expect("nonempty source-fact splice");
            (session.suspend().expect("suspend splice"), root)
        };
        let updated = seal_source_fact_sequence(&mut arena, build, updated_root);
        let updated_sequence = updated.as_ref();
        let mut updated_summary_inspection = SequenceInspectionReceipt::default();
        let updated_measure = updated_sequence
            .summary(&arena, &mut updated_summary_inspection)
            .expect("updated summary")
            .expect("updated root");
        assert_eq!(updated_summary_inspection.node_headers_decoded, 3);
        assert_eq!(updated_summary_inspection.summary_combinations, 1);
        assert!(
            updated_summary_inspection.spec.payload_bytes_inspected
                <= 3 * u64::try_from(ARENA_PAGE_BYTES).expect("page size fits u64")
        );
        assert!(
            updated_summary_inspection.spec.spec_items_hashed
                <= 2 * SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX as u64
        );
        assert_eq!(updated_measure.summary().segment, target_facts.summary());
        assert_eq!(
            updated_measure.leaves(),
            old_facts.page_count() - (suffix_page - restart_page) as u64
                + replacement_facts.page_count()
        );
        assert_eq!(
            receipt.leaves_reused,
            old_facts.pages().len() - (suffix_page - restart_page)
        );
        assert_eq!(receipt.leaves_deleted, suffix_page - restart_page);

        let replacement_pages = replacement_facts.page_count() as usize;
        for (old_index, &base_id) in base_ids.iter().enumerate().take(restart_page) {
            let mut inspection = SequenceInspectionReceipt::default();
            let updated_id = updated_sequence
                .locate_leaf_with_prefix(&arena, old_index as u64, &mut inspection)
                .expect("locate retained prefix")
                .expect("retained prefix")
                .id;
            assert_eq!(updated_id, base_id);
        }
        for (old_index, &base_id) in base_ids.iter().enumerate().skip(suffix_page) {
            let updated_index = restart_page + replacement_pages + (old_index - suffix_page);
            let mut inspection = SequenceInspectionReceipt::default();
            let updated_id = updated_sequence
                .locate_leaf_with_prefix(&arena, updated_index as u64, &mut inspection)
                .expect("locate retained suffix")
                .expect("retained suffix")
                .id;
            assert_eq!(updated_id, base_id);
        }

        // Retiring the old root cannot invalidate the directly retained pages
        // of the new revision; no old-root ancestry is required.
        assert!(base.release(&mut arena).is_ok());
        drain_arena(&mut arena);
        let mut retired_base_inspection = SequenceInspectionReceipt::default();
        assert_eq!(
            updated_sequence
                .summary(&arena, &mut retired_base_inspection)
                .expect("summary after base retirement")
                .expect("root after base retirement")
                .summary()
                .segment,
            target_facts.summary()
        );

        let maximum_gap = spacing as u64 + 1;
        let mut previous_checkpoint = SourceFactCheckpoint::default();
        for ordinal in 0..updated_measure.leaves() {
            let absolute = materialize_source_facts_absolute_page(
                updated_sequence,
                &arena,
                ordinal,
                &mut SequenceInspectionReceipt::default(),
            )
            .expect("materialize updated page")
            .expect("updated absolute page");
            for checkpoint in absolute.checkpoints() {
                assert!(checkpoint.byte_offset > previous_checkpoint.byte_offset);
                assert!(checkpoint.utf16_offset > previous_checkpoint.utf16_offset);
                assert!(checkpoint.utf16_offset - previous_checkpoint.utf16_offset <= maximum_gap);
                assert!(checkpoint.logical_line_breaks >= previous_checkpoint.logical_line_breaks);
                previous_checkpoint = *checkpoint;
            }
        }
        assert_eq!(previous_checkpoint.byte_offset, target_text.len() as u64);
        assert_eq!(
            previous_checkpoint.utf16_offset,
            target_text.encode_utf16().count() as u64
        );
        assert_eq!(
            previous_checkpoint.logical_line_breaks,
            target_facts.logical_line_breaks()
        );
        assert_eq!(
            previous_checkpoint.rolling_hash,
            target_facts.fingerprint().rolling_hash()
        );

        assert!(updated.release(&mut arena).is_ok());
        drain_arena(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
        drop(retired);
    }

    #[test]
    fn persistent_page_codec_rejects_digest_corruption_without_allocating_a_page() {
        let certified = certified(&"ab🌍\r\n".repeat(100), 2);
        let page = certified.facts().pages().next().expect("canonical page");
        let mut payload = [0_u8; ARENA_PAGE_BYTES];
        let encoded = encode_source_fact_sequence_leaf(
            page.canonical(),
            certified.facts().profile(),
            &mut payload,
        )
        .expect("encode page");
        // The fixed header contains the 32-byte content digest before the
        // segment and checkpoint records.
        payload[30] ^= 0x80;
        assert!(matches!(
            decode_persistent_leaf(&payload[..encoded]),
            Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "leaf content digest does not match its checkpoints"
            ))
        ));
    }

    #[test]
    fn persistent_page_codec_rejects_semantic_forgery_with_a_valid_digest() {
        let certified = certified(&"x".repeat(400), 2);
        let profile = certified.facts().profile();
        let page = certified
            .facts()
            .pages()
            .next()
            .expect("source should produce a canonical page")
            .canonical();
        assert!(page.checkpoints().len() > 2);

        let non_monotonic = forged_persistent_leaf_payload(page, profile, |checkpoints| {
            let first = checkpoints[0].summary;
            checkpoints[1].summary.byte_len = first.byte_len;
            checkpoints[1].summary.utf16_len = first.utf16_len;
            checkpoints[1].summary.logical_line_breaks = first.logical_line_breaks;
        });
        assert!(matches!(
            decode_persistent_leaf(&non_monotonic),
            Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "relative checkpoints are not strictly monotonic"
            ))
        ));

        let excessive_gap = forged_persistent_leaf_payload(page, profile, |checkpoints| {
            let forged_gap = profile.checkpoint_spacing_utf16 + 2;
            checkpoints[0].summary.byte_len = forged_gap;
            checkpoints[0].summary.utf16_len = forged_gap;
        });
        assert!(matches!(
            decode_persistent_leaf(&excessive_gap),
            Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "relative checkpoint gap exceeds the scan profile"
            ))
        ));

        let changed_start_flag = forged_persistent_leaf_payload(page, profile, |checkpoints| {
            checkpoints[0].summary.starts_with_line_feed = !checkpoints
                .last()
                .expect("terminal checkpoint")
                .summary
                .starts_with_line_feed;
        });
        assert!(matches!(
            decode_persistent_leaf(&changed_start_flag),
            Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "relative checkpoint start boundary flag changed within a leaf"
            ))
        ));

        let premature_end_flag = forged_persistent_leaf_payload(page, profile, |checkpoints| {
            checkpoints[0].summary.ends_with_carriage_return = true;
        });
        assert!(matches!(
            decode_persistent_leaf(&premature_end_flag),
            Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "non-terminal relative checkpoint carries the leaf end boundary flag"
            ))
        ));

        let impossible_utf8_dimensions =
            forged_persistent_leaf_payload(page, profile, |checkpoints| {
                checkpoints[0].summary.byte_len = 7;
                checkpoints[0].summary.utf16_len = 2;
            });
        assert!(matches!(
            decode_persistent_leaf(&impossible_utf8_dimensions),
            Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "relative checkpoint byte and UTF-16 deltas are impossible"
            ))
        ));

        let impossible_line_delta = forged_persistent_leaf_payload(page, profile, |checkpoints| {
            checkpoints[1].summary.logical_line_breaks = 3;
        });
        assert!(matches!(
            decode_persistent_leaf(&impossible_line_delta),
            Err(SourceFactsAssemblyError::CorruptPersistentSequence(
                "relative checkpoint line delta is impossible"
            ))
        ));
    }

    #[test]
    fn persistent_page_codec_accepts_safe_boundary_spacing_overshoot() {
        for source in ["a🌍z", "a\r\nz"] {
            let certified = certified(source, 2);
            let facts = certified.facts();
            let page = facts
                .pages()
                .next()
                .expect("nonempty source should produce a canonical page");
            assert_eq!(page.canonical().checkpoints()[0].summary().utf16_len(), 3);

            let mut payload = [0_u8; ARENA_PAGE_BYTES];
            let encoded =
                encode_source_fact_sequence_leaf(page.canonical(), facts.profile(), &mut payload)
                    .expect("safe scalar and CRLF overshoot should remain canonical");
            assert!(decode_persistent_leaf(&payload[..encoded])
                .expect("canonical payload should decode")
                .is_some());
        }
    }

    #[test]
    fn empty_source_completes_without_a_checkpoint_page() {
        let (pages, completion) = scan("", 4_096, 1, 1);
        assert!(pages.is_empty());
        assert_eq!(completion.fingerprint().byte_len(), 0);
        assert_eq!(completion.fingerprint().utf16_len(), 0);
        assert_eq!(completion.fingerprint().rolling_hash().words(), [0; 4]);
        assert_eq!(completion.logical_line_breaks(), 0);
        assert_eq!(completion.checkpoint_count(), 0);
        assert_eq!(completion.page_count(), 0);
    }

    #[test]
    fn unicode_and_crlf_match_the_dart_rolling_hash_golden() {
        let source = "a\r\nb\rc\n🌍";
        let (pages, completion) = scan(source, 2, 1, 1);
        assert!(!pages.is_empty());
        assert_eq!(completion.fingerprint().algorithm(), 1);
        assert_eq!(completion.fingerprint().byte_len(), 11);
        assert_eq!(completion.fingerprint().utf16_len(), 9);
        assert_eq!(completion.logical_line_breaks(), 3);
        assert_eq!(
            completion.fingerprint().rolling_hash().words(),
            [612_499_581, 503_726_615, 853_474_825, 1_474_888_379]
        );

        let checkpoints: Vec<_> = pages
            .iter()
            .flat_map(|page| page.checkpoints())
            .copied()
            .collect();
        assert_eq!(
            checkpoints.last().copied(),
            Some(SourceFactCheckpoint {
                byte_offset: 11,
                utf16_offset: 9,
                logical_line_breaks: 3,
                rolling_hash: completion.fingerprint().rolling_hash(),
            })
        );
        assert!(checkpoints.windows(2).all(|pair| {
            pair[0].byte_offset() < pair[1].byte_offset()
                && pair[0].utf16_offset() < pair[1].utf16_offset()
        }));
    }

    #[test]
    fn every_poll_and_page_respects_its_credit() {
        let source = "ab🌍\r\n".repeat(20_000);
        let (pages, completion) = scan(&source, 31, 17, 2);
        assert!(pages.iter().all(|page| page.checkpoints().len() <= 2));
        assert_eq!(completion.fingerprint().byte_len(), source.len() as u64);
        assert_eq!(
            completion.fingerprint().utf16_len(),
            source.encode_utf16().count() as u64
        );
        assert_eq!(completion.logical_line_breaks(), 20_000);
        assert_eq!(completion.page_count(), pages.len() as u64);
        assert_eq!(
            completion.checkpoint_count(),
            pages
                .iter()
                .map(|page| page.checkpoints().len() as u64)
                .sum()
        );
    }

    #[test]
    fn cancellation_releases_the_cursor_and_is_terminal() {
        let store = SourceStore::new(&"x".repeat(100_000)).expect("source should build");
        let mut scanner =
            SourceFactsScanner::new(store.snapshot(), 4_096).expect("scanner should build");
        let first = scanner.poll(7, 1).expect("poll should succeed");
        assert!(matches!(first, SourceFactsPoll::Pending(_)));
        assert!(scanner.cancel());
        assert!(!scanner.cancel());
        assert_eq!(
            scanner.poll(7, 1).expect("cancelled poll should succeed"),
            SourceFactsPoll::Cancelled
        );
    }

    #[test]
    fn dart_fingerprint_vectors_are_exact_across_poll_splits() {
        let vectors = [
            (
                "a😀 café β\n",
                [0xb991_edd9, 0x5fb5_7c47, 0x8873_2115, 0x2292_a46b],
            ),
            (
                "a🌍b\n",
                [0xcc6c_28f6, 0x0aa8_0a4c, 0xdf5f_6342, 0x250a_ffb0],
            ),
            (
                "aé🌍b\n",
                [0x9cfb_81cc, 0x8cb1_defa, 0x6f97_a348, 0x6f98_e8ee],
            ),
            (
                "aéb\n",
                [0x9167_4f8c, 0x5d5a_b6ce, 0xb9ac_ab58, 0x359e_37f2],
            ),
        ];
        for (source, expected) in vectors {
            let mut prior_checkpoints = None;
            let mut prior_digest = None;
            for fuel in 1..=source.len() + 2 {
                let (pages, completion) = scan(source, 2, fuel, 2);
                assert_eq!(completion.fingerprint().rolling_hash().words(), expected);
                let checkpoints: Vec<_> = pages
                    .iter()
                    .flat_map(|page| page.checkpoints())
                    .copied()
                    .collect();
                if let Some(prior) = &prior_checkpoints {
                    assert_eq!(&checkpoints, prior, "fuel={fuel}, source={source:?}");
                }
                if let Some(prior) = prior_digest {
                    assert_eq!(
                        completion.checkpoint_sequence_digest(),
                        prior,
                        "fuel={fuel}, source={source:?}"
                    );
                }
                prior_checkpoints = Some(checkpoints);
                prior_digest = Some(completion.checkpoint_sequence_digest());
            }
        }
    }

    #[test]
    fn lone_carriage_return_runs_keep_bounded_checkpoint_spacing() {
        let source = "\r".repeat(100_000);
        let spacing = 31;
        let (pages, completion) = scan(&source, spacing, 97, 3);
        let checkpoints: Vec<_> = pages
            .iter()
            .flat_map(|page| page.checkpoints())
            .copied()
            .collect();
        assert_eq!(completion.logical_line_breaks(), source.len() as u64);
        assert_eq!(
            checkpoints.last().unwrap().utf16_offset(),
            source.len() as u64
        );
        let mut prior = 0;
        for checkpoint in checkpoints {
            assert!(checkpoint.utf16_offset() - prior <= spacing as u64 + 1);
            prior = checkpoint.utf16_offset();
        }
    }

    #[test]
    fn crlf_and_scalar_boundaries_are_stable_for_every_small_fuel() {
        let source = "\r\n\rX\r\r\né¢ह🌍z\r";
        let baseline = scan(source, 2, SOURCE_FACT_SOURCE_BYTES_PER_POLL_MAX, 64);
        let expected_checkpoints: Vec<_> = baseline
            .0
            .iter()
            .flat_map(|page| page.checkpoints())
            .copied()
            .collect();
        for fuel in 1..=32 {
            let (pages, completion) = scan(source, 2, fuel, 1);
            let checkpoints: Vec<_> = pages
                .iter()
                .flat_map(|page| page.checkpoints())
                .copied()
                .collect();
            assert_eq!(checkpoints, expected_checkpoints, "fuel={fuel}");
            assert_eq!(
                completion.checkpoint_sequence_digest(),
                baseline.1.checkpoint_sequence_digest(),
                "fuel={fuel}"
            );
        }
    }

    #[test]
    fn page_lineage_and_completion_bind_one_scan() {
        let (pages, completion) = scan("abcdefgh", 2, 64, 1);
        assert_eq!(pages.len(), 4);
        for (ordinal, page) in pages.iter().enumerate() {
            assert_eq!(page.scan_id(), completion.scan_id());
            assert_eq!(page.ordinal(), ordinal as u64);
            assert_eq!(
                page.checkpoint_spacing_utf16(),
                completion.checkpoint_spacing_utf16()
            );
        }
        assert_eq!(
            pages.last().unwrap().sequence_digest(),
            completion.checkpoint_sequence_digest()
        );

        let (second_pages, second_completion) = scan("abcdefgh", 2, 1, 1);
        assert_ne!(completion.scan_id(), second_completion.scan_id());
        assert_eq!(
            completion.checkpoint_sequence_digest(),
            second_completion.checkpoint_sequence_digest()
        );
        assert!(second_pages
            .iter()
            .all(|page| page.scan_id() == second_completion.scan_id()));
    }

    #[test]
    fn page_full_at_eof_defers_completion_without_duplicate_checkpoint() {
        let store = SourceStore::new("abcd").expect("source should build");
        let mut scanner =
            SourceFactsScanner::new(store.snapshot(), 2).expect("scanner should build");
        let first = scanner.poll(64, 1).expect("first page should scan");
        let second = scanner.poll(64, 1).expect("second page should scan");
        let completion = scanner.poll(64, 1).expect("completion should follow");
        assert!(matches!(first, SourceFactsPoll::Page { .. }));
        assert!(matches!(second, SourceFactsPoll::Page { .. }));
        match completion {
            SourceFactsPoll::Complete { completion, work } => {
                assert_eq!(completion.checkpoint_count(), 2);
                assert_eq!(work.source_bytes_examined(), 0);
            }
            other => panic!("expected completion, got {other:?}"),
        }
    }

    #[test]
    fn invalid_credits_are_retryable_but_poisoned_scanners_are_terminal() {
        let store = SourceStore::new("live").expect("source should build");
        assert_eq!(
            SourceFactsScanner::new(store.snapshot(), 1).unwrap_err(),
            SourceFactsError::InvalidCheckpointSpacing
        );
        assert_eq!(
            SourceFactsScanner::new(
                store.snapshot(),
                SOURCE_FACT_CHECKPOINT_SPACING_MAX_UTF16 + 1
            )
            .unwrap_err(),
            SourceFactsError::InvalidCheckpointSpacing
        );
        let mut scanner =
            SourceFactsScanner::new(store.snapshot(), 2).expect("scanner should build");
        assert_eq!(scanner.poll(0, 1), Err(SourceFactsError::ZeroFuel));
        assert_eq!(
            scanner.poll(SOURCE_FACT_SOURCE_BYTES_PER_POLL_MAX + 1, 1),
            Err(SourceFactsError::PollLimitExceeded)
        );
        assert_eq!(
            scanner.poll(1, SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX + 1),
            Err(SourceFactsError::PollLimitExceeded)
        );
        assert!(scanner.poll(1, 1).is_ok());
        scanner.poison();
        assert_eq!(scanner.poll(1, 1), Err(SourceFactsError::ScannerPoisoned));
        assert!(!scanner.cancel());
    }

    #[test]
    fn certifier_mints_clean_eof_for_empty_unicode_and_long_sources() {
        for (source, spacing, source_fuel, checkpoint_fuel) in [
            ("".to_owned(), 4_096, 1, 1),
            ("a\r\nb\rc\n🌍".to_owned(), 2, 1, 1),
            ("ab🌍\r\n".repeat(20_000), 31, 97, 3),
        ] {
            let store = SourceStore::new(&source).expect("source should build");
            let lease = store.snapshot();
            let profile = SourceFactsScanProfile::new(spacing).expect("profile should build");
            let (pages, completion) = scan_lease(&lease, profile, source_fuel, checkpoint_fuel);
            let certified = certify(lease, profile, pages, completion)
                .expect("complete exact stream should certify");

            assert_eq!(certified.source(), store.version());
            assert_eq!(certified.coverage(), SourceFactsCoverage::CleanEof);
            assert_eq!(certified.parser_profile(), parser_profile());
            assert_eq!(certified.facts().source(), store.version());
            assert_eq!(certified.facts().profile(), profile);
            assert_eq!(
                certified.facts().fingerprint().byte_len(),
                source.len() as u64
            );
            assert_eq!(
                certified.facts().fingerprint().utf16_len(),
                source.encode_utf16().count() as u64
            );
            if source.is_empty() {
                assert_eq!(certified.facts().page_count(), 0);
                assert_eq!(certified.facts().checkpoint_count(), 0);
            } else {
                let terminal = certified
                    .facts()
                    .pages()
                    .next_back()
                    .expect("non-empty source should have a terminal page")
                    .checkpoints()
                    .last()
                    .expect("non-empty source should have terminal facts");
                assert_eq!(terminal.byte_offset(), source.len() as u64);
                assert_eq!(
                    terminal.utf16_offset(),
                    source.encode_utf16().count() as u64
                );
                assert!(certified
                    .facts()
                    .pages()
                    .all(|page| page.checkpoints().len() <= SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX));
            }
        }
    }

    #[test]
    fn certifier_rejects_swapped_dropped_reordered_and_duplicated_pages() {
        let store = SourceStore::new("abcdefghijklmnop").expect("source should build");
        let lease = store.snapshot();
        let profile = SourceFactsScanProfile::new(2).expect("profile should build");
        let (pages, completion) = scan_lease(&lease, profile, 64, 1);
        let (second_pages, _) = scan_lease(&lease, profile, 1, 1);
        assert!(pages.len() >= 4);

        let mut swapped = root_builder(lease.duplicate(), profile).unwrap();
        swapped.push_page(pages[0].clone()).unwrap();
        assert_eq!(
            swapped.push_page(second_pages[1].clone()),
            Err(SourceFactsAssemblyError::ScanMismatch)
        );
        assert_eq!(
            swapped.certify(completion).unwrap_err(),
            SourceFactsAssemblyError::BuilderPoisoned
        );

        let mut dropped = root_builder(lease.duplicate(), profile).unwrap();
        dropped.push_page(pages[0].clone()).unwrap();
        assert_eq!(
            dropped.push_page(pages[2].clone()),
            Err(SourceFactsAssemblyError::UnexpectedPageOrdinal {
                expected: 1,
                actual: 2,
            })
        );

        let mut reordered = root_builder(lease.duplicate(), profile).unwrap();
        assert_eq!(
            reordered.push_page(pages[1].clone()),
            Err(SourceFactsAssemblyError::UnexpectedPageOrdinal {
                expected: 0,
                actual: 1,
            })
        );

        let mut duplicated = root_builder(lease, profile).unwrap();
        duplicated.push_page(pages[0].clone()).unwrap();
        assert_eq!(
            duplicated.push_page(pages[0].clone()),
            Err(SourceFactsAssemblyError::UnexpectedPageOrdinal {
                expected: 1,
                actual: 0,
            })
        );

        let mut after_terminal = root_builder(store.snapshot(), profile).unwrap();
        for page in pages.iter().cloned() {
            after_terminal.push_page(page).unwrap();
        }
        assert_eq!(
            after_terminal.push_page(pages[0].clone()),
            Err(SourceFactsAssemblyError::PageAfterTerminalCheckpoint)
        );
    }

    #[test]
    fn certifier_rejects_bad_page_digest_counts_and_partial_end_coverage() {
        let store = SourceStore::new("abcdefghijklmnop").expect("source should build");
        let lease = store.snapshot();
        let profile = SourceFactsScanProfile::new(2).expect("profile should build");
        let (pages, completion) = scan_lease(&lease, profile, 64, 1);

        let mut bad_digest_page = pages[0].clone();
        bad_digest_page.sequence_digest.0[0] ^= 1;
        let mut bad_digest_builder = root_builder(lease.duplicate(), profile).unwrap();
        assert_eq!(
            bad_digest_builder.push_page(bad_digest_page),
            Err(SourceFactsAssemblyError::PageDigestMismatch)
        );

        let mut bad_completion_digest = completion;
        bad_completion_digest.checkpoint_sequence_digest.0[0] ^= 1;
        assert_eq!(
            certify(
                lease.duplicate(),
                profile,
                pages.clone(),
                bad_completion_digest,
            )
            .unwrap_err(),
            SourceFactsAssemblyError::CompletionDigestMismatch
        );

        let mut bad_checkpoint_count = completion;
        bad_checkpoint_count.checkpoint_count += 1;
        assert_eq!(
            certify(
                lease.duplicate(),
                profile,
                pages.clone(),
                bad_checkpoint_count,
            )
            .unwrap_err(),
            SourceFactsAssemblyError::CheckpointCountMismatch
        );

        let mut bad_page_count = completion;
        bad_page_count.page_count += 1;
        assert_eq!(
            certify(lease.duplicate(), profile, pages.clone(), bad_page_count).unwrap_err(),
            SourceFactsAssemblyError::PageCountMismatch
        );

        let mut partial_pages = pages.clone();
        let omitted = partial_pages.pop().expect("terminal page should exist");
        let mut false_partial_completion = completion;
        false_partial_completion.page_count -= 1;
        false_partial_completion.checkpoint_count -= omitted.checkpoints.len() as u64;
        false_partial_completion.checkpoint_sequence_digest =
            digest_for_pages(&mut partial_pages, profile.checkpoint_spacing_utf16());
        assert_eq!(
            certify(lease, profile, partial_pages, false_partial_completion).unwrap_err(),
            SourceFactsAssemblyError::MissingTerminalCheckpoint
        );
    }

    #[test]
    fn certifier_rejects_crossed_source_profile_scan_and_terminal_facts() {
        let store = SourceStore::new("a\r\nb🌍c").expect("source should build");
        let other = SourceStore::new("a\r\nb🌍c").expect("source should build");
        let lease = store.snapshot();
        let profile = SourceFactsScanProfile::new(2).expect("profile should build");
        let (pages, completion) = scan_lease(&lease, profile, 3, 2);

        assert_eq!(
            certify(other.snapshot(), profile, pages.clone(), completion).unwrap_err(),
            SourceFactsAssemblyError::SourceMismatch
        );

        let different_profile = SourceFactsScanProfile::new(3).expect("profile should build");
        assert_eq!(
            certify(
                lease.duplicate(),
                different_profile,
                pages.clone(),
                completion,
            )
            .unwrap_err(),
            SourceFactsAssemblyError::ProfileMismatch
        );

        let mut bad_algorithm = completion;
        bad_algorithm.fingerprint.algorithm += 1;
        assert_eq!(
            certify(lease.duplicate(), profile, pages.clone(), bad_algorithm).unwrap_err(),
            SourceFactsAssemblyError::FingerprintAlgorithmMismatch
        );

        let mut bad_dimensions = completion;
        bad_dimensions.fingerprint.byte_len -= 1;
        assert_eq!(
            certify(lease.duplicate(), profile, pages.clone(), bad_dimensions).unwrap_err(),
            SourceFactsAssemblyError::SourceDimensionMismatch
        );

        let mut bad_hash = completion;
        bad_hash.fingerprint.rolling_hash.words[0] ^= 1;
        assert_eq!(
            certify(lease.duplicate(), profile, pages.clone(), bad_hash).unwrap_err(),
            SourceFactsAssemblyError::TerminalFingerprintMismatch
        );

        let mut bad_lines = completion;
        bad_lines.logical_line_breaks += 1;
        assert_eq!(
            certify(lease, profile, pages, bad_lines).unwrap_err(),
            SourceFactsAssemblyError::TerminalLineBreakMismatch
        );
    }

    #[test]
    fn certifier_rejects_checkpoint_corruption_and_numeric_extremes_without_panicking() {
        let store = SourceStore::new("abcdefgh").expect("source should build");
        let lease = store.snapshot();
        let profile = SourceFactsScanProfile::new(2).expect("profile should build");
        let (pages, _) = scan_lease(&lease, profile, 64, 1);

        let mut bad_coordinate = pages[0].clone();
        bad_coordinate.checkpoints[0].byte_offset = u64::MAX;
        let mut builder = root_builder(lease.duplicate(), profile).unwrap();
        assert_eq!(
            builder.push_page(bad_coordinate),
            Err(SourceFactsAssemblyError::CheckpointOutOfBounds)
        );

        let mut oversized = pages[0].clone();
        oversized.checkpoints =
            vec![oversized.checkpoints[0]; SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX + 1]
                .into_boxed_slice();
        let mut builder = root_builder(lease.duplicate(), profile).unwrap();
        assert_eq!(
            builder.push_page(oversized),
            Err(SourceFactsAssemblyError::CheckpointPageTooLarge {
                observed: SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX + 1,
                limit: SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX,
            })
        );

        let mut wrong_coordinate = pages[0].clone();
        wrong_coordinate.checkpoints[0].byte_offset += 1;
        let mut builder = root_builder(lease.duplicate(), profile).unwrap();
        assert_eq!(
            builder.push_page(wrong_coordinate),
            Err(SourceFactsAssemblyError::CheckpointCoordinateMismatch)
        );

        let mut coverage_gap = pages[0].clone();
        coverage_gap.checkpoints[0].byte_offset = 4;
        coverage_gap.checkpoints[0].utf16_offset = 4;
        let mut builder = root_builder(lease.duplicate(), profile).unwrap();
        assert_eq!(
            builder.push_page(coverage_gap),
            Err(SourceFactsAssemblyError::CheckpointCoverageGap)
        );

        let (two_checkpoint_pages, _) = scan_lease(&lease, profile, 64, 2);
        let mut duplicate_checkpoint = two_checkpoint_pages[0].clone();
        duplicate_checkpoint.checkpoints[1] = duplicate_checkpoint.checkpoints[0];
        let mut builder = root_builder(lease, profile).unwrap();
        assert_eq!(
            builder.push_page(duplicate_checkpoint),
            Err(SourceFactsAssemblyError::CheckpointNotMonotonic)
        );
    }

    #[test]
    fn cancelled_or_poisoned_scans_have_no_certification_path() {
        let store = SourceStore::new(&"x".repeat(100_000)).expect("source should build");
        let lease = store.snapshot();
        let profile = SourceFactsScanProfile::new(4_096).expect("profile should build");
        let mut scanner = SourceFactsScanner::with_profile(lease.duplicate(), profile).unwrap();
        assert!(matches!(
            scanner.poll(7, 1).unwrap(),
            SourceFactsPoll::Pending(_)
        ));
        assert!(scanner.cancel());
        assert_eq!(scanner.poll(7, 1).unwrap(), SourceFactsPoll::Cancelled);

        let (pages, completion) = scan_lease(&lease, profile, 64, 1);
        let mut poisoned = root_builder(lease, profile).unwrap();
        let mut malformed = pages[0].clone();
        malformed.ordinal = 1;
        assert!(poisoned.push_page(malformed).is_err());
        assert_eq!(
            poisoned.certify(completion).unwrap_err(),
            SourceFactsAssemblyError::BuilderPoisoned
        );
    }

    #[test]
    fn canonical_root_pages_are_identical_across_every_transport_cut() {
        let source = "a\r\n🌍x".repeat(300);
        let store = SourceStore::new(&source).expect("source should build");
        let profile = SourceFactsScanProfile::new(2).expect("profile should build");
        let mut baseline = None;
        let mut transport_page_counts = Vec::new();

        for source_fuel in [1, 7, 97, SOURCE_FACT_SOURCE_BYTES_PER_POLL_MAX] {
            for checkpoint_fuel in [1, 3, SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX] {
                let lease = store.snapshot();
                let (pages, completion) = scan_lease(&lease, profile, source_fuel, checkpoint_fuel);
                transport_page_counts.push(completion.page_count());
                let certified = certify(lease, profile, pages, completion)
                    .expect("exact stream should certify");
                let signature: Vec<_> = certified
                    .facts()
                    .pages()
                    .map(|page| (page.content_digest().bytes(), page.checkpoints().to_vec()))
                    .collect();
                assert!(signature
                    .iter()
                    .take(signature.len().saturating_sub(1))
                    .all(|(_, checkpoints)| {
                        checkpoints.len() == SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX
                    }));
                assert_eq!(certified.facts().page_count(), signature.len() as u64);
                if let Some(expected) = &baseline {
                    assert_eq!(&signature, expected);
                } else {
                    baseline = Some(signature);
                }
            }
        }

        transport_page_counts.sort_unstable();
        transport_page_counts.dedup();
        assert!(
            transport_page_counts.len() > 1,
            "test must exercise distinct scanner transport cuts"
        );
    }

    #[test]
    fn root_admission_is_explicit_bounded_and_rechecked_while_assembling() {
        let production = SourceFactsScanProfile::new(4_096).expect("production profile");
        let hundred_mib_utf16 = 100 * 1024 * 1024;
        let admitted = SourceFactsRootAdmission::for_utf16_len(
            hundred_mib_utf16,
            production,
            SourceFactsRootLimits::default(),
        )
        .expect("100 MiB production-spaced root should fit bounded defaults");
        assert_eq!(admitted.checkpoint_count(), 25_600);
        assert_eq!(admitted.page_count(), 400);
        assert!(
            admitted.resident_bytes() < SOURCE_FACT_ROOT_DEFAULT_MAX_RESIDENT_BYTES,
            "production admission must remain materially inside its residency cap"
        );

        let dense = SourceFactsScanProfile::new(2).expect("dense test profile");
        assert!(matches!(
            SourceFactsRootAdmission::for_utf16_len(
                hundred_mib_utf16,
                dense,
                SourceFactsRootLimits::default(),
            ),
            Err(SourceFactsAssemblyError::AdmissionCheckpointLimitExceeded { .. })
        ));
        let page_limited = SourceFactsRootLimits::new(30_000, 1, 128 * 1024 * 1024).unwrap();
        assert!(matches!(
            SourceFactsRootAdmission::for_utf16_len(hundred_mib_utf16, production, page_limited,),
            Err(SourceFactsAssemblyError::AdmissionPageLimitExceeded { .. })
        ));
        let byte_limited = SourceFactsRootLimits::new(30_000, 1_000, 1).unwrap();
        assert!(matches!(
            SourceFactsRootAdmission::for_utf16_len(hundred_mib_utf16, production, byte_limited,),
            Err(SourceFactsAssemblyError::AdmissionResidentBytesLimitExceeded { .. })
        ));

        let store = SourceStore::new("abcdefghij").expect("source should build");
        let profile = SourceFactsScanProfile::new(2).expect("profile should build");
        let lease = store.snapshot();
        let (mut pages, _) = scan_lease(&lease, profile, 64, 64);
        let mut prefix_hash = SourceContentHash128::default();
        prefix_hash.append_byte(b'a');
        let injected = SourceFactCheckpoint {
            byte_offset: 1,
            utf16_offset: 1,
            logical_line_breaks: 0,
            rolling_hash: prefix_hash,
        };
        let mut checkpoints = pages[0].checkpoints.to_vec();
        checkpoints.insert(0, injected);
        pages[0].checkpoints = checkpoints.into_boxed_slice();
        let _ = digest_for_pages(&mut pages, profile.checkpoint_spacing_utf16());
        let mut builder = root_builder(lease, profile).expect("preflight should admit source");
        assert!(matches!(
            builder.push_page(pages.remove(0)),
            Err(SourceFactsAssemblyError::AdmissionCheckpointLimitExceeded {
                needed: 6,
                limit: 5,
            })
        ));
    }
}
