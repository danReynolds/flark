//! Parser-owned persistent projection payload for top-level indented code.
//!
//! This schema records physical source geometry only. Every record covers one
//! complete physical line in an exact, at-most-8-KiB window of one immutable
//! indented-code block. Internal blank lines remain in the payload; leading and
//! trailing blank lines belong to separate exact-clean `Blank` leaves.
//!
//! The semantic final LF required by CommonMark is descriptor metadata. It is
//! never represented as source and therefore can never acquire caret affinity.

use std::fmt;
use std::ops::Range;

use crate::identity::ArenaId;
use crate::measured_sequence::maximum_avl_height;
use crate::parser_pages::{
    imported_m11_parser_page_record_at, validate_imported_m11_parser_page_root,
    M11ImportedParserPageRootClaim, M11ParserPageBuild, M11ParserPageBuildReceipt,
    M11ParserPageBuildStatus, M11ParserPageCursor, M11ParserPageCursorPoll, M11ParserPageError,
    M11ParserPageReclaimPoll, M11ParserPageRecord, M11ParserPageRoot,
    M11_PARSER_PAGE_MAX_RECORD_BYTES,
};
use crate::storage::PageArena;
use crate::{DocumentRuntime, ParserProfileId, SourceSnapshotLease, SourceVersion};

const STREAM_TAG: u32 = u32::from_le_bytes(*b"ICP1");
const SCHEMA: u32 = 1;
const PAGE_MAGIC: [u8; 4] = *b"ICP1";
const PAGE_HEADER_BYTES: usize = 16;
/// Canonical byte width of one [`IndentedCodeLineV1`].
pub const INDENTED_CODE_LINE_V1_BYTES: usize = 20;
/// Maximum authoritative physical source window accepted by this urgent path.
pub const INDENTED_CODE_WINDOW_MAX_BYTES: usize = 8 * 1024;
/// Maximum line records in one parser-defined logical page.
pub const INDENTED_CODE_LINES_PER_PAGE_MAX: usize =
    (M11_PARSER_PAGE_MAX_RECORD_BYTES - PAGE_HEADER_BYTES) / INDENTED_CODE_LINE_V1_BYTES;

const COMMITMENT_DOMAIN: &[u8] = b"flark.indented-code-projection.v1\0";
const COMMITMENT_TRAILER: &[u8] = b"flark.indented-code-projection.end.v1\0";
const PERSISTENT_DESCRIPTOR_MAGIC: [u8; 4] = *b"ICR1";
const PERSISTENT_DESCRIPTOR_SCHEMA: u32 = 1;
pub(crate) const PERSISTENT_INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES: usize = 160;

/// The physical line is a blank line inside two nonblank code lines.
pub const INDENTED_CODE_LINE_FLAG_INTERNAL_BLANK: u32 = 1;
const KNOWN_LINE_FLAGS: u32 = INDENTED_CODE_LINE_FLAG_INTERNAL_BLANK;

/// The semantic code literal ends in one synthetic LF.
pub const INDENTED_CODE_PROJECTION_FLAG_SYNTHETIC_FINAL_LF: u32 = 1;

/// One canonical physical-line projection record.
///
/// `relative_line_start` is relative to the descriptor's physical block
/// start. `content_length` names source-backed literal bytes only; the physical
/// EOL and descriptor-owned semantic final LF are separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndentedCodeLineV1 {
    relative_line_start: u32,
    physical_source_length: u32,
    hidden_prefix_length: u32,
    content_length: u32,
    flags: u32,
}

impl IndentedCodeLineV1 {
    /// Creates and validates one top-level indented-code record.
    pub fn new(
        relative_line_start: u32,
        physical_source_length: u32,
        hidden_prefix_length: u32,
        content_length: u32,
        flags: u32,
    ) -> Result<Self, M11IndentedCodeProjectionError> {
        let line = Self {
            relative_line_start,
            physical_source_length,
            hidden_prefix_length,
            content_length,
            flags,
        };
        validate_line(line)?;
        Ok(line)
    }

    /// Convenience constructor for one nonblank code line.
    pub fn code(
        relative_line_start: u32,
        physical_source_length: u32,
        hidden_prefix_length: u32,
        content_length: u32,
    ) -> Result<Self, M11IndentedCodeProjectionError> {
        Self::new(
            relative_line_start,
            physical_source_length,
            hidden_prefix_length,
            content_length,
            0,
        )
    }

    /// Convenience constructor for a source-backed internal blank line.
    pub fn internal_blank(
        relative_line_start: u32,
        physical_source_length: u32,
        hidden_prefix_length: u32,
    ) -> Result<Self, M11IndentedCodeProjectionError> {
        Self::new(
            relative_line_start,
            physical_source_length,
            hidden_prefix_length,
            0,
            INDENTED_CODE_LINE_FLAG_INTERNAL_BLANK,
        )
    }

    #[must_use]
    pub const fn relative_line_start(self) -> u32 {
        self.relative_line_start
    }

    #[must_use]
    pub const fn physical_source_length(self) -> u32 {
        self.physical_source_length
    }

    #[must_use]
    pub const fn hidden_prefix_length(self) -> u32 {
        self.hidden_prefix_length
    }

    #[must_use]
    pub const fn content_length(self) -> u32 {
        self.content_length
    }

    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }

    #[must_use]
    pub const fn is_internal_blank(self) -> bool {
        self.flags & INDENTED_CODE_LINE_FLAG_INTERNAL_BLANK != 0
    }

    #[must_use]
    pub const fn physical_eol_length(self) -> u32 {
        self.physical_source_length
            .saturating_sub(self.hidden_prefix_length)
            .saturating_sub(self.content_length)
    }

    pub fn relative_source_range(self) -> Result<Range<u32>, M11IndentedCodeProjectionError> {
        Ok(self.relative_line_start
            ..self
                .relative_line_start
                .checked_add(self.physical_source_length)
                .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?)
    }

    pub fn absolute_source_range(
        self,
        descriptor: &M11IndentedCodeProjectionDescriptor,
    ) -> Result<Range<u32>, M11IndentedCodeProjectionError> {
        let start = descriptor
            .physical_block_range
            .start
            .checked_add(self.relative_line_start)
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        Ok(start
            ..start
                .checked_add(self.physical_source_length)
                .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?)
    }

    pub fn absolute_content_range(
        self,
        descriptor: &M11IndentedCodeProjectionDescriptor,
    ) -> Result<Range<u32>, M11IndentedCodeProjectionError> {
        let source = self.absolute_source_range(descriptor)?;
        let start = source
            .start
            .checked_add(self.hidden_prefix_length)
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        Ok(start
            ..start
                .checked_add(self.content_length)
                .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?)
    }
}

/// Exact authority and authenticated summary of one projection window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11IndentedCodeProjectionDescriptor {
    source: SourceVersion,
    parser_profile: ParserProfileId,
    physical_block_range: Range<u32>,
    requested_window: Range<u32>,
    projection_flags: u32,
    logical_page_count: u64,
    line_count: u64,
    storage_page_count: u64,
    storage_payload_bytes: u64,
    storage_encoded_bytes: u64,
    storage_checksum256: [u8; 32],
    ordered_commitment256: [u8; 32],
}

impl M11IndentedCodeProjectionDescriptor {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn parser_profile(&self) -> ParserProfileId {
        self.parser_profile
    }

    #[must_use]
    pub const fn physical_block_range(&self) -> &Range<u32> {
        &self.physical_block_range
    }

    #[must_use]
    pub const fn requested_window(&self) -> &Range<u32> {
        &self.requested_window
    }

    #[must_use]
    pub const fn projection_flags(&self) -> u32 {
        self.projection_flags
    }

    #[must_use]
    pub const fn has_synthetic_final_lf(&self) -> bool {
        self.projection_flags & INDENTED_CODE_PROJECTION_FLAG_SYNTHETIC_FINAL_LF != 0
    }

    #[must_use]
    pub const fn logical_page_count(&self) -> u64 {
        self.logical_page_count
    }

    #[must_use]
    pub const fn line_count(&self) -> u64 {
        self.line_count
    }

    #[must_use]
    pub const fn storage_page_count(&self) -> u64 {
        self.storage_page_count
    }

    #[must_use]
    pub const fn storage_payload_bytes(&self) -> u64 {
        self.storage_payload_bytes
    }

    #[must_use]
    pub const fn storage_encoded_bytes(&self) -> u64 {
        self.storage_encoded_bytes
    }

    #[must_use]
    pub const fn storage_checksum256(&self) -> [u8; 32] {
        self.storage_checksum256
    }

    #[must_use]
    pub const fn ordered_commitment256(&self) -> [u8; 32] {
        self.ordered_commitment256
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentM11IndentedCodeProjectionDescriptor {
    source: SourceVersion,
    parser_profile: ParserProfileId,
    block_start: u32,
    block_end: u32,
    window_start: u32,
    window_end: u32,
    projection_flags: u32,
    logical_page_count: u64,
    line_count: u64,
    storage_page_count: u64,
    payload_bytes: u64,
    encoded_bytes: u64,
    checksum: [u8; 32],
    ordered_commitment256: [u8; 32],
}

impl PersistentM11IndentedCodeProjectionDescriptor {
    pub(crate) const fn source(self) -> SourceVersion {
        self.source
    }

    pub(crate) const fn parser_profile(self) -> ParserProfileId {
        self.parser_profile
    }

    pub(crate) fn physical_block_range(self) -> Range<u32> {
        self.block_start..self.block_end
    }

    pub(crate) fn requested_window(self) -> Range<u32> {
        self.window_start..self.window_end
    }

    pub(crate) const fn projection_flags(self) -> u32 {
        self.projection_flags
    }

    pub(crate) const fn logical_page_count(self) -> u64 {
        self.logical_page_count
    }

    pub(crate) const fn line_count(self) -> u64 {
        self.line_count
    }

    pub(crate) const fn storage_page_count(self) -> u64 {
        self.storage_page_count
    }

    pub(crate) const fn ordered_commitment256(self) -> [u8; 32] {
        self.ordered_commitment256
    }

    pub(crate) fn maximum_query_open_depth(self) -> u32 {
        u32::from(maximum_avl_height(self.storage_page_count)).saturating_add(1)
    }

    pub(crate) fn maximum_query_tree_nodes_visited(self) -> Option<u64> {
        let height = u64::from(maximum_avl_height(self.storage_page_count));
        self.logical_page_count
            .checked_mul(height.checked_mul(3)?.checked_add(6)?)
    }

    fn public_descriptor(self) -> M11IndentedCodeProjectionDescriptor {
        M11IndentedCodeProjectionDescriptor {
            source: self.source,
            parser_profile: self.parser_profile,
            physical_block_range: self.physical_block_range(),
            requested_window: self.requested_window(),
            projection_flags: self.projection_flags,
            logical_page_count: self.logical_page_count,
            line_count: self.line_count,
            storage_page_count: self.storage_page_count,
            storage_payload_bytes: self.payload_bytes,
            storage_encoded_bytes: self.encoded_bytes,
            storage_checksum256: self.checksum,
            ordered_commitment256: self.ordered_commitment256,
        }
    }

    fn page_claim(self) -> M11ImportedParserPageRootClaim {
        M11ImportedParserPageRootClaim {
            stream_tag: STREAM_TAG,
            storage_page_count: self.storage_page_count,
            record_count: self.logical_page_count,
            payload_bytes: self.payload_bytes,
            encoded_bytes: self.encoded_bytes,
            checksum: self.checksum,
        }
    }
}

/// Canonical-schema, authority, lifecycle, or page failure.
#[derive(Debug)]
pub enum M11IndentedCodeProjectionError {
    InvalidAuthority(&'static str),
    WindowTooLarge { bytes: usize, cap: usize },
    EmptyLogicalPage,
    TooManyLines { lines: usize, cap: usize },
    InvalidLine(&'static str),
    CoverageMismatch,
    LeadingBlankLine,
    TrailingBlankLine,
    CoordinateOverflow,
    SourceAuthorityMismatch,
    ParserProfileMismatch,
    PhysicalBlockMismatch,
    RequestedWindowMismatch,
    InvalidState,
    CommitmentMismatch,
    Malformed(&'static str),
    Pages(M11ParserPageError),
}

impl fmt::Display for M11IndentedCodeProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAuthority(message) => {
                write!(formatter, "invalid indented-code authority: {message}")
            }
            Self::WindowTooLarge { bytes, cap } => write!(
                formatter,
                "indented-code window has {bytes} bytes above the {cap}-byte cap"
            ),
            Self::EmptyLogicalPage => {
                formatter.write_str("indented-code logical pages must not be empty")
            }
            Self::TooManyLines { lines, cap } => write!(
                formatter,
                "indented-code page has {lines} lines above the {cap}-line cap"
            ),
            Self::InvalidLine(message) => {
                write!(formatter, "invalid indented-code line: {message}")
            }
            Self::CoverageMismatch => formatter
                .write_str("indented-code lines do not exhaustively tile the requested window"),
            Self::LeadingBlankLine => {
                formatter.write_str("indented-code payload includes a leading blank leaf")
            }
            Self::TrailingBlankLine => {
                formatter.write_str("indented-code payload includes a trailing blank leaf")
            }
            Self::CoordinateOverflow => formatter.write_str("indented-code coordinate overflow"),
            Self::SourceAuthorityMismatch => {
                formatter.write_str("indented-code source authority mismatch")
            }
            Self::ParserProfileMismatch => {
                formatter.write_str("indented-code parser profile mismatch")
            }
            Self::PhysicalBlockMismatch => {
                formatter.write_str("indented-code physical block authority mismatch")
            }
            Self::RequestedWindowMismatch => {
                formatter.write_str("indented-code requested window authority mismatch")
            }
            Self::InvalidState => {
                formatter.write_str("indented-code projection owner is in the wrong state")
            }
            Self::CommitmentMismatch => {
                formatter.write_str("indented-code ordered commitment mismatch")
            }
            Self::Malformed(message) => write!(
                formatter,
                "malformed indented-code projection page: {message}"
            ),
            Self::Pages(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11IndentedCodeProjectionError {}

impl From<M11ParserPageError> for M11IndentedCodeProjectionError {
    fn from(value: M11ParserPageError) -> Self {
        Self::Pages(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11IndentedCodeProjectionBuildStatus {
    NeedsPage,
    Pending,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11IndentedCodeProjectionBuildPoll {
    status: M11IndentedCodeProjectionBuildStatus,
    transitions: usize,
}

impl M11IndentedCodeProjectionBuildPoll {
    #[must_use]
    pub const fn status(self) -> M11IndentedCodeProjectionBuildStatus {
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
    Finishing,
    Complete,
    Cancelled,
    Failed,
}

/// Move-only, fuelled builder for one exact top-level indented-code window.
#[must_use = "indented-code builds require root transfer or explicit cancellation"]
pub struct M11IndentedCodeProjectionBuild {
    inner: M11ParserPageBuild,
    source: SourceVersion,
    parser_profile: ParserProfileId,
    physical_block_range: Range<u32>,
    requested_window: Range<u32>,
    phase: BuildPhase,
    next_relative_start: u32,
    last_line_blank: Option<bool>,
    last_line_eol_length: Option<u32>,
    saw_eof_line: bool,
    logical_page_count: u64,
    line_count: u64,
    stream_hasher: blake3::Hasher,
    ordered_commitment256: Option<[u8; 32]>,
    output: Option<M11IndentedCodeProjectionRoot>,
    failed_root: Option<M11ParserPageRoot>,
}

impl fmt::Debug for M11IndentedCodeProjectionBuild {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11IndentedCodeProjectionBuild")
            .field("source", &self.source)
            .field("parser_profile", &self.parser_profile)
            .field("physical_block_range", &self.physical_block_range)
            .field("requested_window", &self.requested_window)
            .field("phase", &self.phase)
            .field("logical_page_count", &self.logical_page_count)
            .field("line_count", &self.line_count)
            .finish_non_exhaustive()
    }
}

impl M11IndentedCodeProjectionBuild {
    pub fn new(
        runtime: &DocumentRuntime,
        lease: SourceSnapshotLease,
        physical_block_range: Range<usize>,
        requested_window: Range<usize>,
        parser_profile: ParserProfileId,
    ) -> Result<Self, M11IndentedCodeProjectionError> {
        validate_authority_ranges(&lease, &physical_block_range, &requested_window)?;
        let source = lease.version();
        let block = range_u32(&physical_block_range)?;
        let window = range_u32(&requested_window)?;
        let next_relative_start = window
            .start
            .checked_sub(block.start)
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        let inner = M11ParserPageBuild::new(runtime, lease, physical_block_range, STREAM_TAG)?;
        Ok(Self {
            inner,
            source,
            parser_profile,
            physical_block_range: block.clone(),
            requested_window: window.clone(),
            phase: BuildPhase::Accepting,
            next_relative_start,
            last_line_blank: None,
            last_line_eol_length: None,
            saw_eof_line: false,
            logical_page_count: 0,
            line_count: 0,
            stream_hasher: begin_commitment(source, parser_profile, &block, &window),
            ordered_commitment256: None,
            output: None,
            failed_root: None,
        })
    }

    /// Offers one explicit logical page of source-ordered physical lines.
    pub fn offer_page(
        &mut self,
        lines: &[IndentedCodeLineV1],
    ) -> Result<(), M11IndentedCodeProjectionError> {
        if self.phase != BuildPhase::Accepting {
            return Err(M11IndentedCodeProjectionError::InvalidState);
        }
        if lines.is_empty() {
            return Err(M11IndentedCodeProjectionError::EmptyLogicalPage);
        }
        if lines.len() > INDENTED_CODE_LINES_PER_PAGE_MAX {
            return Err(M11IndentedCodeProjectionError::TooManyLines {
                lines: lines.len(),
                cap: INDENTED_CODE_LINES_PER_PAGE_MAX,
            });
        }

        let window_end_relative = self
            .requested_window
            .end
            .checked_sub(self.physical_block_range.start)
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        let mut next = self.next_relative_start;
        let mut last_blank = self.last_line_blank;
        let mut last_eol = self.last_line_eol_length;
        let mut saw_eof = self.saw_eof_line;
        let mut page_physical_bytes = 0_u32;
        for line in lines {
            validate_line(*line)?;
            if line.relative_line_start != next || saw_eof {
                return Err(M11IndentedCodeProjectionError::CoverageMismatch);
            }
            let end = next
                .checked_add(line.physical_source_length)
                .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
            if end > window_end_relative {
                return Err(M11IndentedCodeProjectionError::CoverageMismatch);
            }
            if self.line_count == 0
                && last_blank.is_none()
                && self.requested_window.start == self.physical_block_range.start
                && line.is_internal_blank()
            {
                return Err(M11IndentedCodeProjectionError::LeadingBlankLine);
            }
            if line.physical_eol_length() == 0 {
                let absolute_end = self
                    .physical_block_range
                    .start
                    .checked_add(end)
                    .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
                if absolute_end
                    != u32::try_from(self.source.byte_len())
                        .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?
                {
                    return Err(M11IndentedCodeProjectionError::InvalidLine(
                        "EOF ending does not terminate the immutable source",
                    ));
                }
                saw_eof = true;
            }
            next = end;
            last_blank = Some(line.is_internal_blank());
            last_eol = Some(line.physical_eol_length());
            page_physical_bytes = page_physical_bytes
                .checked_add(line.physical_source_length)
                .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        }

        let encoded = encode_page(lines, page_physical_bytes)?;
        self.inner
            .offer_record(M11ParserPageRecord::new(encoded.as_bytes())?)?;
        append_page_to_commitment(&mut self.stream_hasher, encoded.as_bytes());
        self.next_relative_start = next;
        self.last_line_blank = last_blank;
        self.last_line_eol_length = last_eol;
        self.saw_eof_line = saw_eof;
        self.logical_page_count = self
            .logical_page_count
            .checked_add(1)
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        self.line_count = self
            .line_count
            .checked_add(
                u64::try_from(lines.len())
                    .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?,
            )
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        Ok(())
    }

    pub fn finish_input(&mut self) -> Result<(), M11IndentedCodeProjectionError> {
        if self.phase != BuildPhase::Accepting {
            return Err(M11IndentedCodeProjectionError::InvalidState);
        }
        let expected_end = self
            .requested_window
            .end
            .checked_sub(self.physical_block_range.start)
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        if self.line_count == 0 || self.next_relative_start != expected_end {
            return Err(M11IndentedCodeProjectionError::CoverageMismatch);
        }
        if self.requested_window.end == self.physical_block_range.end
            && self.last_line_blank == Some(true)
        {
            return Err(M11IndentedCodeProjectionError::TrailingBlankLine);
        }
        self.inner.finish_input()?;
        let projection_flags = terminal_projection_flags(
            &self.physical_block_range,
            &self.requested_window,
            self.last_line_eol_length,
        )?;
        self.ordered_commitment256 = Some(finish_commitment(
            &self.stream_hasher,
            self.logical_page_count,
            self.line_count,
            projection_flags,
        ));
        self.phase = BuildPhase::Finishing;
        Ok(())
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11IndentedCodeProjectionBuildPoll, M11IndentedCodeProjectionError> {
        if self.phase == BuildPhase::Failed {
            return Err(M11IndentedCodeProjectionError::InvalidState);
        }
        let poll = self.inner.poll(runtime, fuel)?;
        let status = match poll.status() {
            M11ParserPageBuildStatus::NeedsInput => {
                if self.phase != BuildPhase::Accepting {
                    self.phase = BuildPhase::Failed;
                    return Err(M11IndentedCodeProjectionError::InvalidState);
                }
                M11IndentedCodeProjectionBuildStatus::NeedsPage
            }
            M11ParserPageBuildStatus::Pending => M11IndentedCodeProjectionBuildStatus::Pending,
            M11ParserPageBuildStatus::Cancelled => {
                self.phase = BuildPhase::Cancelled;
                M11IndentedCodeProjectionBuildStatus::Cancelled
            }
            M11ParserPageBuildStatus::Complete => {
                if self.output.is_none() {
                    self.complete_root()?;
                }
                self.phase = BuildPhase::Complete;
                M11IndentedCodeProjectionBuildStatus::Complete
            }
        };
        Ok(M11IndentedCodeProjectionBuildPoll {
            status,
            transitions: poll.transitions(),
        })
    }

    fn complete_root(&mut self) -> Result<(), M11IndentedCodeProjectionError> {
        let commitment = self
            .ordered_commitment256
            .ok_or(M11IndentedCodeProjectionError::InvalidState)?;
        let root = self
            .inner
            .take_root()
            .ok_or(M11IndentedCodeProjectionError::InvalidState)?;
        let source_range = root.source_range();
        let exact = root.source() == self.source
            && source_range.start
                == usize::try_from(self.physical_block_range.start)
                    .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?
            && source_range.end
                == usize::try_from(self.physical_block_range.end)
                    .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?
            && root.stream_tag() == STREAM_TAG
            && root.record_count() == self.logical_page_count;
        if !exact {
            self.failed_root = Some(root);
            self.phase = BuildPhase::Failed;
            return Err(M11IndentedCodeProjectionError::Malformed(
                "generic page root changed typed authority",
            ));
        }
        let projection_flags = terminal_projection_flags(
            &self.physical_block_range,
            &self.requested_window,
            self.last_line_eol_length,
        )?;
        let descriptor = M11IndentedCodeProjectionDescriptor {
            source: self.source,
            parser_profile: self.parser_profile,
            physical_block_range: self.physical_block_range.clone(),
            requested_window: self.requested_window.clone(),
            projection_flags,
            logical_page_count: self.logical_page_count,
            line_count: self.line_count,
            storage_page_count: root.page_count(),
            storage_payload_bytes: root.payload_bytes(),
            storage_encoded_bytes: root.encoded_bytes(),
            storage_checksum256: root.checksum(),
            ordered_commitment256: commitment,
        };
        self.output = Some(M11IndentedCodeProjectionRoot {
            inner: root,
            descriptor,
        });
        Ok(())
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11IndentedCodeProjectionError> {
        if let Some(output) = self.output.as_mut() {
            output.begin_release(runtime)?;
        }
        if let Some(root) = self.failed_root.as_mut() {
            root.begin_release(runtime)?;
        }
        self.inner.begin_cancel(runtime)?;
        self.phase = BuildPhase::Cancelled;
        Ok(())
    }

    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ParserPageReclaimPoll, M11IndentedCodeProjectionError> {
        if self.phase != BuildPhase::Cancelled {
            return Err(M11IndentedCodeProjectionError::InvalidState);
        }
        let poll = self.inner.poll_cancel(runtime, fuel)?;
        if poll.complete() {
            self.output.take();
            self.failed_root.take();
        }
        Ok(poll)
    }

    #[must_use]
    pub fn take_root(&mut self) -> Option<M11IndentedCodeProjectionRoot> {
        if self.phase != BuildPhase::Complete {
            return None;
        }
        self.output.take()
    }

    #[must_use]
    pub fn build_receipt(&self) -> M11ParserPageBuildReceipt {
        self.inner.receipt()
    }
}

/// Move-only persistent root for one exact indented-code projection window.
#[must_use = "indented-code roots require transfer or explicit fuelled release"]
pub struct M11IndentedCodeProjectionRoot {
    inner: M11ParserPageRoot,
    descriptor: M11IndentedCodeProjectionDescriptor,
}

impl fmt::Debug for M11IndentedCodeProjectionRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11IndentedCodeProjectionRoot")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl M11IndentedCodeProjectionRoot {
    #[must_use]
    pub const fn descriptor(&self) -> &M11IndentedCodeProjectionDescriptor {
        &self.descriptor
    }

    /// Returns the authority-free arena closure root and canonical 160-byte
    /// descriptor used by the independent host snapshot transport.
    pub(crate) fn transport_parts(
        &self,
        runtime: &DocumentRuntime,
        expected_source: SourceVersion,
        expected_profile: ParserProfileId,
    ) -> Result<
        (
            Option<ArenaId>,
            [u8; PERSISTENT_INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES],
        ),
        M11IndentedCodeProjectionError,
    > {
        if expected_source != self.descriptor.source {
            return Err(M11IndentedCodeProjectionError::SourceAuthorityMismatch);
        }
        if expected_profile != self.descriptor.parser_profile {
            return Err(M11IndentedCodeProjectionError::ParserProfileMismatch);
        }
        let _ = self.inner.cursor(runtime)?;
        Ok((
            self.inner.transport_root_id()?,
            encode_persistent_descriptor(&self.descriptor)?,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn cursor<'root>(
        &'root self,
        runtime: &DocumentRuntime,
        expected_source: SourceVersion,
        expected_profile: ParserProfileId,
        expected_physical_block_range: Range<u32>,
        expected_requested_window: Range<u32>,
    ) -> Result<M11IndentedCodeProjectionCursor<'root>, M11IndentedCodeProjectionError> {
        if expected_source != self.descriptor.source {
            return Err(M11IndentedCodeProjectionError::SourceAuthorityMismatch);
        }
        if expected_profile != self.descriptor.parser_profile {
            return Err(M11IndentedCodeProjectionError::ParserProfileMismatch);
        }
        if expected_physical_block_range != self.descriptor.physical_block_range {
            return Err(M11IndentedCodeProjectionError::PhysicalBlockMismatch);
        }
        if expected_requested_window != self.descriptor.requested_window {
            return Err(M11IndentedCodeProjectionError::RequestedWindowMismatch);
        }
        let start = self
            .descriptor
            .requested_window
            .start
            .checked_sub(self.descriptor.physical_block_range.start)
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        Ok(M11IndentedCodeProjectionCursor {
            inner: self.inner.cursor(runtime)?,
            descriptor: &self.descriptor,
            hasher: begin_commitment(
                self.descriptor.source,
                self.descriptor.parser_profile,
                &self.descriptor.physical_block_range,
                &self.descriptor.requested_window,
            ),
            next_relative_start: start,
            last_line_blank: None,
            last_line_eol_length: None,
            saw_eof_line: false,
            observed_pages: 0,
            observed_lines: 0,
            current_page: None,
            complete: false,
        })
    }

    pub fn begin_release(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11IndentedCodeProjectionError> {
        self.inner.begin_release(runtime)?;
        Ok(())
    }

    pub fn poll_release(
        &self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ParserPageReclaimPoll, M11IndentedCodeProjectionError> {
        Ok(self.inner.poll_release(runtime, fuel)?)
    }
}

#[derive(Debug)]
pub enum M11IndentedCodeProjectionCursorPoll {
    Pending {
        transitions: usize,
    },
    Line {
        transitions: usize,
        line: IndentedCodeLineV1,
    },
    Complete {
        transitions: usize,
    },
}

struct LoadedPage {
    record: M11ParserPageRecord,
    next_line: usize,
    line_count: usize,
}

/// Typed validating cursor over one immutable projection root.
pub struct M11IndentedCodeProjectionCursor<'root> {
    inner: M11ParserPageCursor<'root>,
    descriptor: &'root M11IndentedCodeProjectionDescriptor,
    hasher: blake3::Hasher,
    next_relative_start: u32,
    last_line_blank: Option<bool>,
    last_line_eol_length: Option<u32>,
    saw_eof_line: bool,
    observed_pages: u64,
    observed_lines: u64,
    current_page: Option<LoadedPage>,
    complete: bool,
}

impl M11IndentedCodeProjectionCursor<'_> {
    pub fn poll(
        &mut self,
        runtime: &DocumentRuntime,
    ) -> Result<M11IndentedCodeProjectionCursorPoll, M11IndentedCodeProjectionError> {
        if self.complete {
            return Ok(M11IndentedCodeProjectionCursorPoll::Complete { transitions: 0 });
        }
        if let Some(page) = self.current_page.as_mut() {
            if page.next_line < page.line_count {
                let line = decode_line(page.record.as_bytes(), page.next_line)?;
                page.next_line += 1;
                return Ok(M11IndentedCodeProjectionCursorPoll::Line {
                    transitions: 1,
                    line,
                });
            }
            self.current_page = None;
        }

        match self.inner.poll(runtime)? {
            M11ParserPageCursorPoll::Pending { transitions } => {
                Ok(M11IndentedCodeProjectionCursorPoll::Pending { transitions })
            }
            M11ParserPageCursorPoll::Record {
                transitions,
                record,
            } => {
                let decoded = validate_page(
                    record.as_bytes(),
                    self.descriptor,
                    self.next_relative_start,
                    self.last_line_blank,
                    self.saw_eof_line,
                    self.observed_lines,
                )?;
                append_page_to_commitment(&mut self.hasher, record.as_bytes());
                self.next_relative_start = decoded.next_relative_start;
                self.last_line_blank = Some(decoded.last_line_blank);
                self.last_line_eol_length = Some(decoded.last_line_eol_length);
                self.saw_eof_line = decoded.saw_eof_line;
                self.observed_pages = self
                    .observed_pages
                    .checked_add(1)
                    .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
                self.observed_lines = self
                    .observed_lines
                    .checked_add(
                        u64::try_from(decoded.line_count)
                            .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?,
                    )
                    .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
                let line = decode_line(record.as_bytes(), 0)?;
                self.current_page = Some(LoadedPage {
                    record,
                    next_line: 1,
                    line_count: decoded.line_count,
                });
                Ok(M11IndentedCodeProjectionCursorPoll::Line { transitions, line })
            }
            M11ParserPageCursorPoll::Complete { transitions } => {
                validate_terminal_replay(
                    self.descriptor,
                    self.next_relative_start,
                    self.last_line_blank,
                    self.last_line_eol_length,
                    self.observed_pages,
                    self.observed_lines,
                )?;
                let actual = finish_commitment(
                    &self.hasher,
                    self.observed_pages,
                    self.observed_lines,
                    self.descriptor.projection_flags,
                );
                if actual != self.descriptor.ordered_commitment256 {
                    return Err(M11IndentedCodeProjectionError::CommitmentMismatch);
                }
                self.complete = true;
                Ok(M11IndentedCodeProjectionCursorPoll::Complete { transitions })
            }
        }
    }

    #[must_use]
    pub const fn descriptor(&self) -> &M11IndentedCodeProjectionDescriptor {
        self.descriptor
    }
}

fn encode_persistent_descriptor(
    descriptor: &M11IndentedCodeProjectionDescriptor,
) -> Result<
    [u8; PERSISTENT_INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES],
    M11IndentedCodeProjectionError,
> {
    let source_bytes = u32::try_from(descriptor.source.byte_len())
        .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?;
    let source_utf16 = u32::try_from(descriptor.source.utf16_len())
        .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?;
    let line_count = u32::try_from(descriptor.line_count)
        .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?;
    let mut output = [0_u8; PERSISTENT_INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES];
    let mut cursor = 0;
    let mut write = |bytes: &[u8]| {
        let end = cursor + bytes.len();
        output[cursor..end].copy_from_slice(bytes);
        cursor = end;
    };
    write(&PERSISTENT_DESCRIPTOR_MAGIC);
    write(&PERSISTENT_DESCRIPTOR_SCHEMA.to_le_bytes());
    write(&descriptor.source.root().get().to_le_bytes());
    write(&descriptor.source.revision().get().to_le_bytes());
    write(&source_bytes.to_le_bytes());
    write(&source_utf16.to_le_bytes());
    write(&descriptor.parser_profile.get().to_le_bytes());
    write(&descriptor.physical_block_range.start.to_le_bytes());
    write(&descriptor.physical_block_range.end.to_le_bytes());
    write(&descriptor.requested_window.start.to_le_bytes());
    write(&descriptor.requested_window.end.to_le_bytes());
    write(&descriptor.projection_flags.to_le_bytes());
    write(&line_count.to_le_bytes());
    write(&descriptor.logical_page_count.to_le_bytes());
    write(&descriptor.storage_page_count.to_le_bytes());
    write(&descriptor.storage_payload_bytes.to_le_bytes());
    write(&descriptor.storage_encoded_bytes.to_le_bytes());
    write(&descriptor.storage_checksum256);
    write(&descriptor.ordered_commitment256);
    debug_assert_eq!(cursor, PERSISTENT_INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES);
    Ok(output)
}

pub(crate) fn decode_persistent_indented_code_projection_descriptor(
    bytes: &[u8],
    expected_source: SourceVersion,
    expected_profile: ParserProfileId,
) -> Result<PersistentM11IndentedCodeProjectionDescriptor, M11IndentedCodeProjectionError> {
    let expected_source_bytes = u32::try_from(expected_source.byte_len())
        .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?;
    let expected_source_utf16 = u32::try_from(expected_source.utf16_len())
        .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?;
    if bytes.len() != PERSISTENT_INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES
        || bytes[..4] != PERSISTENT_DESCRIPTOR_MAGIC
        || read_u32(bytes, 4)? != PERSISTENT_DESCRIPTOR_SCHEMA
        || read_u64(bytes, 8)? != expected_source.root().get()
        || read_u64(bytes, 16)? != expected_source.revision().get()
        || read_u32(bytes, 24)? != expected_source_bytes
        || read_u32(bytes, 28)? != expected_source_utf16
    {
        return Err(M11IndentedCodeProjectionError::SourceAuthorityMismatch);
    }
    if read_u64(bytes, 32)? != expected_profile.get() {
        return Err(M11IndentedCodeProjectionError::ParserProfileMismatch);
    }
    let block_start = read_u32(bytes, 40)?;
    let block_end = read_u32(bytes, 44)?;
    let window_start = read_u32(bytes, 48)?;
    let window_end = read_u32(bytes, 52)?;
    let projection_flags = read_u32(bytes, 56)?;
    let line_count = u64::from(read_u32(bytes, 60)?);
    let logical_page_count = read_u64(bytes, 64)?;
    let storage_page_count = read_u64(bytes, 72)?;
    let payload_bytes = read_u64(bytes, 80)?;
    let encoded_bytes = read_u64(bytes, 88)?;
    let checksum: [u8; 32] = bytes[96..128]
        .try_into()
        .expect("fixed indented-code checksum");
    let ordered_commitment256: [u8; 32] = bytes[128..160]
        .try_into()
        .expect("fixed indented-code commitment");
    let window_bytes = window_end.checked_sub(window_start);
    if projection_flags & !INDENTED_CODE_PROJECTION_FLAG_SYNTHETIC_FINAL_LF != 0
        || block_start >= block_end
        || window_start < block_start
        || window_start >= window_end
        || window_end > block_end
        || window_bytes.is_none_or(|value| value as usize > INDENTED_CODE_WINDOW_MAX_BYTES)
        || usize::try_from(block_end)
            .ok()
            .is_none_or(|end| end > expected_source.byte_len())
        || line_count == 0
        || logical_page_count == 0
        || logical_page_count > line_count
        || storage_page_count == 0
        || payload_bytes == 0
        || encoded_bytes == 0
    {
        return Err(M11IndentedCodeProjectionError::Malformed(
            "persistent descriptor dimensions are invalid",
        ));
    }
    Ok(PersistentM11IndentedCodeProjectionDescriptor {
        source: expected_source,
        parser_profile: expected_profile,
        block_start,
        block_end,
        window_start,
        window_end,
        projection_flags,
        logical_page_count,
        line_count,
        storage_page_count,
        payload_bytes,
        encoded_bytes,
        checksum,
        ordered_commitment256,
    })
}

pub(crate) fn validate_persistent_indented_code_projection_root(
    arena: &PageArena,
    root: Option<ArenaId>,
    descriptor_bytes: &[u8],
    expected_source: SourceVersion,
    expected_profile: ParserProfileId,
) -> Result<PersistentM11IndentedCodeProjectionDescriptor, M11IndentedCodeProjectionError> {
    let descriptor = decode_persistent_indented_code_projection_descriptor(
        descriptor_bytes,
        expected_source,
        expected_profile,
    )?;
    validate_imported_m11_parser_page_root(arena, root, descriptor.page_claim())?;
    Ok(descriptor)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentM11IndentedCodeProjectionHostValidationPoll {
    pub(crate) transitions: usize,
    pub(crate) complete: bool,
}

/// Fuelled typed validation required before an imported closure is installed.
///
/// Generic admission authenticates the opaque page tree. This pass derives
/// line coverage and terminal semantics from every canonical `ICP1` record,
/// then authenticates the ordered BLAKE3 commitment.
pub(crate) struct PersistentM11IndentedCodeProjectionHostValidator {
    root: Option<ArenaId>,
    descriptor: PersistentM11IndentedCodeProjectionDescriptor,
    public_descriptor: M11IndentedCodeProjectionDescriptor,
    hasher: blake3::Hasher,
    next_relative_start: u32,
    last_line_blank: Option<bool>,
    last_line_eol_length: Option<u32>,
    saw_eof_line: bool,
    observed_pages: u64,
    observed_lines: u64,
    complete: bool,
}

impl PersistentM11IndentedCodeProjectionHostValidator {
    pub(crate) fn new(
        arena: &PageArena,
        root: Option<ArenaId>,
        descriptor: PersistentM11IndentedCodeProjectionDescriptor,
    ) -> Result<Self, M11IndentedCodeProjectionError> {
        validate_imported_m11_parser_page_root(arena, root, descriptor.page_claim())?;
        let public_descriptor = descriptor.public_descriptor();
        let next_relative_start = descriptor
            .window_start
            .checked_sub(descriptor.block_start)
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        Ok(Self {
            root,
            descriptor,
            public_descriptor,
            hasher: begin_commitment(
                descriptor.source,
                descriptor.parser_profile,
                &(descriptor.block_start..descriptor.block_end),
                &(descriptor.window_start..descriptor.window_end),
            ),
            next_relative_start,
            last_line_blank: None,
            last_line_eol_length: None,
            saw_eof_line: false,
            observed_pages: 0,
            observed_lines: 0,
            complete: false,
        })
    }

    pub(crate) fn poll(
        &mut self,
        arena: &PageArena,
        fuel: usize,
    ) -> Result<PersistentM11IndentedCodeProjectionHostValidationPoll, M11IndentedCodeProjectionError>
    {
        if fuel == 0 {
            return Err(M11IndentedCodeProjectionError::Pages(
                M11ParserPageError::ZeroFuel,
            ));
        }
        if self.complete {
            return Ok(PersistentM11IndentedCodeProjectionHostValidationPoll {
                transitions: 0,
                complete: true,
            });
        }
        let mut transitions = 0;
        while transitions < fuel {
            if self.observed_pages < self.descriptor.logical_page_count {
                let record = imported_m11_parser_page_record_at(
                    arena,
                    self.root,
                    self.descriptor.page_claim(),
                    self.observed_pages,
                )?;
                let decoded = validate_page(
                    record.as_bytes(),
                    &self.public_descriptor,
                    self.next_relative_start,
                    self.last_line_blank,
                    self.saw_eof_line,
                    self.observed_lines,
                )?;
                append_page_to_commitment(&mut self.hasher, record.as_bytes());
                self.next_relative_start = decoded.next_relative_start;
                self.last_line_blank = Some(decoded.last_line_blank);
                self.last_line_eol_length = Some(decoded.last_line_eol_length);
                self.saw_eof_line = decoded.saw_eof_line;
                self.observed_pages = self
                    .observed_pages
                    .checked_add(1)
                    .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
                self.observed_lines = self
                    .observed_lines
                    .checked_add(
                        u64::try_from(decoded.line_count)
                            .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?,
                    )
                    .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
                transitions += 1;
                continue;
            }
            validate_terminal_replay(
                &self.public_descriptor,
                self.next_relative_start,
                self.last_line_blank,
                self.last_line_eol_length,
                self.observed_pages,
                self.observed_lines,
            )?;
            let commitment = finish_commitment(
                &self.hasher,
                self.observed_pages,
                self.observed_lines,
                self.descriptor.projection_flags,
            );
            if commitment != self.descriptor.ordered_commitment256 {
                return Err(M11IndentedCodeProjectionError::CommitmentMismatch);
            }
            self.complete = true;
            transitions += 1;
            break;
        }
        Ok(PersistentM11IndentedCodeProjectionHostValidationPoll {
            transitions,
            complete: self.complete,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PersistentM11IndentedCodeProjectionHostCursorPoll {
    Line { line: IndentedCodeLineV1 },
    Complete,
}

/// Installed-only typed replay over an independently validated arena closure.
pub(crate) struct PersistentM11IndentedCodeProjectionHostCursor<'arena> {
    arena: &'arena PageArena,
    root: Option<ArenaId>,
    descriptor: PersistentM11IndentedCodeProjectionDescriptor,
    public_descriptor: M11IndentedCodeProjectionDescriptor,
    hasher: blake3::Hasher,
    next_page: u64,
    next_relative_start: u32,
    last_line_blank: Option<bool>,
    last_line_eol_length: Option<u32>,
    saw_eof_line: bool,
    observed_lines: u64,
    current_page: Option<LoadedPage>,
    complete: bool,
}

impl<'arena> PersistentM11IndentedCodeProjectionHostCursor<'arena> {
    pub(crate) fn new(
        arena: &'arena PageArena,
        root: Option<ArenaId>,
        descriptor: PersistentM11IndentedCodeProjectionDescriptor,
    ) -> Self {
        let public_descriptor = descriptor.public_descriptor();
        let next_relative_start = descriptor.window_start - descriptor.block_start;
        Self {
            arena,
            root,
            descriptor,
            public_descriptor,
            hasher: begin_commitment(
                descriptor.source,
                descriptor.parser_profile,
                &(descriptor.block_start..descriptor.block_end),
                &(descriptor.window_start..descriptor.window_end),
            ),
            next_page: 0,
            next_relative_start,
            last_line_blank: None,
            last_line_eol_length: None,
            saw_eof_line: false,
            observed_lines: 0,
            current_page: None,
            complete: false,
        }
    }

    pub(crate) fn poll(
        &mut self,
    ) -> Result<PersistentM11IndentedCodeProjectionHostCursorPoll, M11IndentedCodeProjectionError>
    {
        if self.complete {
            return Ok(PersistentM11IndentedCodeProjectionHostCursorPoll::Complete);
        }
        if let Some(page) = self.current_page.as_mut() {
            if page.next_line < page.line_count {
                let line = decode_line(page.record.as_bytes(), page.next_line)?;
                page.next_line += 1;
                return Ok(PersistentM11IndentedCodeProjectionHostCursorPoll::Line { line });
            }
            self.current_page = None;
        }
        if self.next_page == self.descriptor.logical_page_count {
            validate_terminal_replay(
                &self.public_descriptor,
                self.next_relative_start,
                self.last_line_blank,
                self.last_line_eol_length,
                self.next_page,
                self.observed_lines,
            )?;
            let actual = finish_commitment(
                &self.hasher,
                self.next_page,
                self.observed_lines,
                self.descriptor.projection_flags,
            );
            if actual != self.descriptor.ordered_commitment256 {
                return Err(M11IndentedCodeProjectionError::CommitmentMismatch);
            }
            self.complete = true;
            return Ok(PersistentM11IndentedCodeProjectionHostCursorPoll::Complete);
        }
        let record = imported_m11_parser_page_record_at(
            self.arena,
            self.root,
            self.descriptor.page_claim(),
            self.next_page,
        )?;
        let decoded = validate_page(
            record.as_bytes(),
            &self.public_descriptor,
            self.next_relative_start,
            self.last_line_blank,
            self.saw_eof_line,
            self.observed_lines,
        )?;
        append_page_to_commitment(&mut self.hasher, record.as_bytes());
        self.next_page = self
            .next_page
            .checked_add(1)
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        self.next_relative_start = decoded.next_relative_start;
        self.last_line_blank = Some(decoded.last_line_blank);
        self.last_line_eol_length = Some(decoded.last_line_eol_length);
        self.saw_eof_line = decoded.saw_eof_line;
        self.observed_lines = self
            .observed_lines
            .checked_add(
                u64::try_from(decoded.line_count)
                    .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?,
            )
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        let line = decode_line(record.as_bytes(), 0)?;
        self.current_page = Some(LoadedPage {
            record,
            next_line: 1,
            line_count: decoded.line_count,
        });
        Ok(PersistentM11IndentedCodeProjectionHostCursorPoll::Line { line })
    }

    pub(crate) const fn tree_nodes_visited(&self) -> u64 {
        self.next_page
    }
}

struct EncodedPage {
    bytes: [u8; M11_PARSER_PAGE_MAX_RECORD_BYTES],
    len: usize,
}

impl EncodedPage {
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

fn encode_page(
    lines: &[IndentedCodeLineV1],
    page_physical_bytes: u32,
) -> Result<EncodedPage, M11IndentedCodeProjectionError> {
    let len = PAGE_HEADER_BYTES
        .checked_add(
            lines
                .len()
                .checked_mul(INDENTED_CODE_LINE_V1_BYTES)
                .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?,
        )
        .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
    if lines.is_empty()
        || lines.len() > INDENTED_CODE_LINES_PER_PAGE_MAX
        || len > M11_PARSER_PAGE_MAX_RECORD_BYTES
    {
        return Err(M11IndentedCodeProjectionError::Malformed(
            "logical page dimensions are invalid",
        ));
    }
    let mut bytes = [0_u8; M11_PARSER_PAGE_MAX_RECORD_BYTES];
    bytes[..4].copy_from_slice(&PAGE_MAGIC);
    bytes[4..8].copy_from_slice(&SCHEMA.to_le_bytes());
    bytes[8..10].copy_from_slice(
        &u16::try_from(lines.len())
            .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?
            .to_le_bytes(),
    );
    bytes[12..16].copy_from_slice(&page_physical_bytes.to_le_bytes());
    for (ordinal, line) in lines.iter().enumerate() {
        let start = PAGE_HEADER_BYTES + ordinal * INDENTED_CODE_LINE_V1_BYTES;
        bytes[start..start + 4].copy_from_slice(&line.relative_line_start.to_le_bytes());
        bytes[start + 4..start + 8].copy_from_slice(&line.physical_source_length.to_le_bytes());
        bytes[start + 8..start + 12].copy_from_slice(&line.hidden_prefix_length.to_le_bytes());
        bytes[start + 12..start + 16].copy_from_slice(&line.content_length.to_le_bytes());
        bytes[start + 16..start + 20].copy_from_slice(&line.flags.to_le_bytes());
    }
    Ok(EncodedPage { bytes, len })
}

struct DecodedPage {
    line_count: usize,
    next_relative_start: u32,
    last_line_blank: bool,
    last_line_eol_length: u32,
    saw_eof_line: bool,
}

fn validate_page(
    bytes: &[u8],
    descriptor: &M11IndentedCodeProjectionDescriptor,
    expected_start: u32,
    previous_blank: Option<bool>,
    previous_eof: bool,
    observed_lines: u64,
) -> Result<DecodedPage, M11IndentedCodeProjectionError> {
    if bytes.get(..4) != Some(PAGE_MAGIC.as_slice()) || read_u32(bytes, 4)? != SCHEMA {
        return Err(M11IndentedCodeProjectionError::Malformed(
            "logical page magic or schema is unsupported",
        ));
    }
    let line_count = usize::from(read_u16(bytes, 8)?);
    if read_u16(bytes, 10)? != 0
        || line_count == 0
        || line_count > INDENTED_CODE_LINES_PER_PAGE_MAX
        || bytes.len() != PAGE_HEADER_BYTES + line_count * INDENTED_CODE_LINE_V1_BYTES
    {
        return Err(M11IndentedCodeProjectionError::Malformed(
            "logical page dimensions are invalid",
        ));
    }
    let expected_window_end = descriptor
        .requested_window
        .end
        .checked_sub(descriptor.physical_block_range.start)
        .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
    let mut next = expected_start;
    let mut blank = previous_blank;
    let mut last_eol = None;
    let mut saw_eof = previous_eof;
    let mut physical_bytes = 0_u32;
    for ordinal in 0..line_count {
        let line = decode_line(bytes, ordinal)?;
        validate_line(line)?;
        if line.relative_line_start != next || saw_eof {
            return Err(M11IndentedCodeProjectionError::CoverageMismatch);
        }
        let end = next
            .checked_add(line.physical_source_length)
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
        if end > expected_window_end {
            return Err(M11IndentedCodeProjectionError::CoverageMismatch);
        }
        if observed_lines == 0
            && ordinal == 0
            && descriptor.requested_window.start == descriptor.physical_block_range.start
            && line.is_internal_blank()
        {
            return Err(M11IndentedCodeProjectionError::LeadingBlankLine);
        }
        if line.physical_eol_length() == 0 {
            let absolute_end = descriptor
                .physical_block_range
                .start
                .checked_add(end)
                .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
            if absolute_end
                != u32::try_from(descriptor.source.byte_len())
                    .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?
            {
                return Err(M11IndentedCodeProjectionError::InvalidLine(
                    "EOF ending does not terminate the immutable source",
                ));
            }
            saw_eof = true;
        }
        next = end;
        blank = Some(line.is_internal_blank());
        last_eol = Some(line.physical_eol_length());
        physical_bytes = physical_bytes
            .checked_add(line.physical_source_length)
            .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
    }
    if physical_bytes != read_u32(bytes, 12)? {
        return Err(M11IndentedCodeProjectionError::Malformed(
            "logical page physical byte total changed",
        ));
    }
    Ok(DecodedPage {
        line_count,
        next_relative_start: next,
        last_line_blank: blank.expect("nonempty page"),
        last_line_eol_length: last_eol.expect("nonempty page"),
        saw_eof_line: saw_eof,
    })
}

fn validate_terminal_replay(
    descriptor: &M11IndentedCodeProjectionDescriptor,
    next_relative_start: u32,
    last_line_blank: Option<bool>,
    last_line_eol_length: Option<u32>,
    observed_pages: u64,
    observed_lines: u64,
) -> Result<(), M11IndentedCodeProjectionError> {
    let expected_end = descriptor
        .requested_window
        .end
        .checked_sub(descriptor.physical_block_range.start)
        .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
    if next_relative_start != expected_end
        || observed_pages != descriptor.logical_page_count
        || observed_lines != descriptor.line_count
        || observed_lines == 0
    {
        return Err(M11IndentedCodeProjectionError::CoverageMismatch);
    }
    if descriptor.requested_window.end == descriptor.physical_block_range.end
        && last_line_blank == Some(true)
    {
        return Err(M11IndentedCodeProjectionError::TrailingBlankLine);
    }
    let expected_flags = terminal_projection_flags(
        &descriptor.physical_block_range,
        &descriptor.requested_window,
        last_line_eol_length,
    )?;
    if descriptor.projection_flags != expected_flags {
        return Err(M11IndentedCodeProjectionError::Malformed(
            "projection semantic flags are unsupported",
        ));
    }
    Ok(())
}

fn decode_line(
    bytes: &[u8],
    ordinal: usize,
) -> Result<IndentedCodeLineV1, M11IndentedCodeProjectionError> {
    let start = PAGE_HEADER_BYTES
        .checked_add(
            ordinal
                .checked_mul(INDENTED_CODE_LINE_V1_BYTES)
                .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?,
        )
        .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
    let record = bytes
        .get(start..start + INDENTED_CODE_LINE_V1_BYTES)
        .ok_or(M11IndentedCodeProjectionError::Malformed(
            "line record is truncated",
        ))?;
    Ok(IndentedCodeLineV1 {
        relative_line_start: read_u32(record, 0)?,
        physical_source_length: read_u32(record, 4)?,
        hidden_prefix_length: read_u32(record, 8)?,
        content_length: read_u32(record, 12)?,
        flags: read_u32(record, 16)?,
    })
}

fn validate_line(line: IndentedCodeLineV1) -> Result<(), M11IndentedCodeProjectionError> {
    if line.flags & !KNOWN_LINE_FLAGS != 0 {
        return Err(M11IndentedCodeProjectionError::InvalidLine(
            "unknown flag bits are set",
        ));
    }
    if line.physical_source_length == 0 {
        return Err(M11IndentedCodeProjectionError::InvalidLine(
            "physical source length must be nonzero",
        ));
    }
    let prefix_and_content = line
        .hidden_prefix_length
        .checked_add(line.content_length)
        .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
    if prefix_and_content > line.physical_source_length {
        return Err(M11IndentedCodeProjectionError::InvalidLine(
            "hidden prefix and content exceed physical source length",
        ));
    }
    let eol = line.physical_source_length - prefix_and_content;
    if eol > 2 {
        return Err(M11IndentedCodeProjectionError::InvalidLine(
            "physical EOL length must be zero, one, or two bytes",
        ));
    }
    if line.is_internal_blank() {
        if line.content_length != 0 || !(1..=2).contains(&eol) {
            return Err(M11IndentedCodeProjectionError::InvalidLine(
                "internal blank must have no content and a one- or two-byte EOL",
            ));
        }
    } else if line.content_length == 0 || line.hidden_prefix_length == 0 {
        return Err(M11IndentedCodeProjectionError::InvalidLine(
            "nonblank code line requires hidden prefix and content bytes",
        ));
    }
    line.relative_line_start
        .checked_add(line.physical_source_length)
        .ok_or(M11IndentedCodeProjectionError::CoordinateOverflow)?;
    Ok(())
}

fn validate_authority_ranges(
    lease: &SourceSnapshotLease,
    block: &Range<usize>,
    window: &Range<usize>,
) -> Result<(), M11IndentedCodeProjectionError> {
    if block.start >= block.end
        || block.end > lease.version().byte_len()
        || window.start >= window.end
        || window.start < block.start
        || window.end > block.end
    {
        return Err(M11IndentedCodeProjectionError::InvalidAuthority(
            "block/window ranges are empty, reversed, outside source, or not nested",
        ));
    }
    let bytes = window.end - window.start;
    if bytes > INDENTED_CODE_WINDOW_MAX_BYTES {
        return Err(M11IndentedCodeProjectionError::WindowTooLarge {
            bytes,
            cap: INDENTED_CODE_WINDOW_MAX_BYTES,
        });
    }
    for boundary in [block.start, window.start] {
        let scalar = lease.utf16_offset_for_byte(boundary).is_ok();
        let line = lease.is_physical_line_start(boundary).unwrap_or(false);
        if !scalar || !line {
            return Err(M11IndentedCodeProjectionError::InvalidAuthority(
                "block/window cuts must be complete physical-line boundaries",
            ));
        }
    }
    for boundary in [block.end, window.end] {
        let scalar = lease.utf16_offset_for_byte(boundary).is_ok();
        let line = boundary == lease.version().byte_len()
            || lease.is_physical_line_start(boundary).unwrap_or(false);
        if !scalar || !line {
            return Err(M11IndentedCodeProjectionError::InvalidAuthority(
                "block/window cuts must be complete physical-line boundaries",
            ));
        }
    }
    if u32::try_from(lease.version().byte_len()).is_err() {
        return Err(M11IndentedCodeProjectionError::CoordinateOverflow);
    }
    Ok(())
}

fn range_u32(range: &Range<usize>) -> Result<Range<u32>, M11IndentedCodeProjectionError> {
    Ok(
        u32::try_from(range.start)
            .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?
            ..u32::try_from(range.end)
                .map_err(|_| M11IndentedCodeProjectionError::CoordinateOverflow)?,
    )
}

fn begin_commitment(
    source: SourceVersion,
    parser_profile: ParserProfileId,
    block: &Range<u32>,
    window: &Range<u32>,
) -> blake3::Hasher {
    let mut hasher = blake3::Hasher::new();
    hasher.update(COMMITMENT_DOMAIN);
    hasher.update(&SCHEMA.to_le_bytes());
    hasher.update(&parser_profile.get().to_le_bytes());
    hasher.update(&source.root().get().to_le_bytes());
    hasher.update(&source.revision().get().to_le_bytes());
    hasher.update(&(source.byte_len() as u64).to_le_bytes());
    hasher.update(&(source.utf16_len() as u64).to_le_bytes());
    hasher.update(&block.start.to_le_bytes());
    hasher.update(&block.end.to_le_bytes());
    hasher.update(&window.start.to_le_bytes());
    hasher.update(&window.end.to_le_bytes());
    hasher
}

fn append_page_to_commitment(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn finish_commitment(
    hasher: &blake3::Hasher,
    pages: u64,
    lines: u64,
    projection_flags: u32,
) -> [u8; 32] {
    let mut hasher = hasher.clone();
    hasher.update(COMMITMENT_TRAILER);
    hasher.update(&pages.to_le_bytes());
    hasher.update(&lines.to_le_bytes());
    hasher.update(&projection_flags.to_le_bytes());
    *hasher.finalize().as_bytes()
}

fn terminal_projection_flags(
    block: &Range<u32>,
    window: &Range<u32>,
    last_line_eol_length: Option<u32>,
) -> Result<u32, M11IndentedCodeProjectionError> {
    let eol = last_line_eol_length.ok_or(M11IndentedCodeProjectionError::CoverageMismatch)?;
    Ok(if window.end == block.end && eol == 0 {
        INDENTED_CODE_PROJECTION_FLAG_SYNTHETIC_FINAL_LF
    } else {
        0
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, M11IndentedCodeProjectionError> {
    let bytes = bytes
        .get(offset..offset + 2)
        .ok_or(M11IndentedCodeProjectionError::Malformed(
            "u16 is truncated",
        ))?;
    Ok(u16::from_le_bytes(
        bytes.try_into().expect("checked u16 width"),
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, M11IndentedCodeProjectionError> {
    let bytes = bytes
        .get(offset..offset + 4)
        .ok_or(M11IndentedCodeProjectionError::Malformed(
            "u32 is truncated",
        ))?;
    Ok(u32::from_le_bytes(
        bytes.try_into().expect("checked u32 width"),
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, M11IndentedCodeProjectionError> {
    let bytes = bytes
        .get(offset..offset + 8)
        .ok_or(M11IndentedCodeProjectionError::Malformed(
            "u64 is truncated",
        ))?;
    Ok(u64::from_le_bytes(
        bytes.try_into().expect("checked u64 width"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocumentRuntimeConfig;

    fn profile(value: u64) -> ParserProfileId {
        ParserProfileId::new(value).expect("profile")
    }

    fn accept_page(
        build: &mut M11IndentedCodeProjectionBuild,
        runtime: &mut DocumentRuntime,
        lines: &[IndentedCodeLineV1],
    ) {
        build.offer_page(lines).expect("offer page");
        loop {
            match build.poll(runtime, 16).expect("poll page").status() {
                M11IndentedCodeProjectionBuildStatus::NeedsPage => break,
                M11IndentedCodeProjectionBuildStatus::Pending => {}
                other => panic!("unexpected build state {other:?}"),
            }
        }
    }

    fn finish(
        build: &mut M11IndentedCodeProjectionBuild,
        runtime: &mut DocumentRuntime,
    ) -> M11IndentedCodeProjectionRoot {
        build.finish_input().expect("finish input");
        loop {
            match build.poll(runtime, 16).expect("poll finish").status() {
                M11IndentedCodeProjectionBuildStatus::Pending => {}
                M11IndentedCodeProjectionBuildStatus::Complete => {
                    return build.take_root().expect("root");
                }
                other => panic!("unexpected finish state {other:?}"),
            }
        }
    }

    fn release(root: &mut M11IndentedCodeProjectionRoot, runtime: &mut DocumentRuntime) {
        root.begin_release(runtime).expect("begin release");
        while !root
            .poll_release(runtime, 16)
            .expect("poll release")
            .complete()
        {}
    }

    fn close(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin close");
        while !runtime.poll_close(64).expect("poll close").complete {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    fn canonical_lines() -> [IndentedCodeLineV1; 3] {
        [
            IndentedCodeLineV1::code(0, 11, 4, 5).expect("first"),
            IndentedCodeLineV1::internal_blank(11, 1, 0).expect("blank"),
            IndentedCodeLineV1::code(12, 5, 1, 4).expect("last"),
        ]
    }

    #[test]
    fn canonical_window_round_trips_with_exact_authority_and_commitment() {
        let source = "    alpha\r\n\n\tbeta";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let version = runtime.current_source_version().expect("source");
        let mut build = M11IndentedCodeProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("lease"),
            0..source.len(),
            0..source.len(),
            profile(7),
        )
        .expect("build");
        let lines = canonical_lines();
        accept_page(&mut build, &mut runtime, &lines[..2]);
        accept_page(&mut build, &mut runtime, &lines[2..]);
        let mut root = finish(&mut build, &mut runtime);
        let descriptor = root.descriptor();
        assert_eq!(descriptor.line_count(), 3);
        assert_eq!(descriptor.logical_page_count(), 2);
        assert!(descriptor.has_synthetic_final_lf());
        assert_ne!(descriptor.ordered_commitment256(), [0; 32]);

        let mut cursor = root
            .cursor(
                &runtime,
                version,
                profile(7),
                0..source.len() as u32,
                0..source.len() as u32,
            )
            .expect("cursor");
        let mut replay = Vec::new();
        loop {
            match cursor.poll(&runtime).expect("cursor poll") {
                M11IndentedCodeProjectionCursorPoll::Pending { .. } => {}
                M11IndentedCodeProjectionCursorPoll::Line { line, .. } => replay.push(line),
                M11IndentedCodeProjectionCursorPoll::Complete { .. } => break,
            }
        }
        assert_eq!(replay, lines);
        drop(cursor);
        release(&mut root, &mut runtime);
        drop(root);
        close(runtime);
    }

    #[test]
    fn synthetic_final_lf_is_only_for_terminal_eof_without_source_eol() {
        for (source, block, window, line, expected_synthetic) in [
            (
                "    a",
                0..5,
                0..5,
                IndentedCodeLineV1::code(0, 5, 4, 1).expect("EOF line"),
                true,
            ),
            (
                "    a\n",
                0..6,
                0..6,
                IndentedCodeLineV1::code(0, 6, 4, 1).expect("LF line"),
                false,
            ),
            (
                "    a\n    b\n",
                0..12,
                0..6,
                IndentedCodeLineV1::code(0, 6, 4, 1).expect("interior line"),
                false,
            ),
        ] {
            let mut runtime =
                DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
            let mut build = M11IndentedCodeProjectionBuild::new(
                &runtime,
                runtime.snapshot_current_source().expect("lease"),
                block,
                window,
                profile(8),
            )
            .expect("build");
            accept_page(&mut build, &mut runtime, &[line]);
            let mut root = finish(&mut build, &mut runtime);
            assert_eq!(
                root.descriptor().has_synthetic_final_lf(),
                expected_synthetic
            );
            release(&mut root, &mut runtime);
            drop(root);
            close(runtime);
        }
    }

    #[test]
    fn transport_descriptor_is_160_bytes_and_independent_host_replays_typed_lines() {
        let source = "    alpha\r\n\n\tbeta";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let version = runtime.current_source_version().expect("source");
        let lines = canonical_lines();
        let mut build = M11IndentedCodeProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("lease"),
            0..source.len(),
            0..source.len(),
            profile(9),
        )
        .expect("build");
        accept_page(&mut build, &mut runtime, &lines);
        let mut root = finish(&mut build, &mut runtime);
        let (arena_root, descriptor_bytes) = root
            .transport_parts(&runtime, version, profile(9))
            .expect("transport parts");
        assert_eq!(
            descriptor_bytes.len(),
            PERSISTENT_INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES
        );
        let descriptor = validate_persistent_indented_code_projection_root(
            runtime.producer_arena(),
            arena_root,
            &descriptor_bytes,
            version,
            profile(9),
        )
        .expect("typed imported root");
        assert_eq!(descriptor.line_count(), 3);
        let mut validator = PersistentM11IndentedCodeProjectionHostValidator::new(
            runtime.producer_arena(),
            arena_root,
            descriptor,
        )
        .expect("host validator");
        let mut validation_transitions = 0;
        loop {
            let poll = validator
                .poll(runtime.producer_arena(), 1)
                .expect("fuelled typed validation");
            assert!(poll.transitions <= 1);
            validation_transitions += poll.transitions;
            if poll.complete {
                break;
            }
        }
        assert_eq!(
            validation_transitions,
            descriptor.logical_page_count() as usize + 1
        );
        drop(validator);
        let mut cursor = PersistentM11IndentedCodeProjectionHostCursor::new(
            runtime.producer_arena(),
            arena_root,
            descriptor,
        );
        let mut replay = Vec::new();
        while let PersistentM11IndentedCodeProjectionHostCursorPoll::Line { line } =
            cursor.poll().expect("host cursor")
        {
            replay.push(line);
        }
        assert_eq!(replay, lines);
        drop(cursor);
        release(&mut root, &mut runtime);
        drop(root);
        close(runtime);
    }

    #[test]
    fn line_schema_rejects_bad_geometry_unknown_flags_and_internal_eof() {
        assert!(IndentedCodeLineV1::new(0, 10, 4, 5, 0).is_ok());
        assert!(matches!(
            IndentedCodeLineV1::new(0, 12, 4, 5, 0),
            Err(M11IndentedCodeProjectionError::InvalidLine(_))
        ));
        assert!(matches!(
            IndentedCodeLineV1::new(0, 1, 0, 0, INDENTED_CODE_LINE_FLAG_INTERNAL_BLANK | 2,),
            Err(M11IndentedCodeProjectionError::InvalidLine(
                "unknown flag bits are set"
            ))
        ));
        assert!(matches!(
            IndentedCodeLineV1::new(0, 1, 1, 0, INDENTED_CODE_LINE_FLAG_INTERNAL_BLANK,),
            Err(M11IndentedCodeProjectionError::InvalidLine(
                "internal blank must have no content and a one- or two-byte EOL"
            ))
        ));
    }

    #[test]
    fn builder_rejects_gaps_and_a_terminal_internal_blank() {
        let source = "    a\n\n";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let mut gap = M11IndentedCodeProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("lease"),
            0..source.len(),
            0..source.len(),
            profile(1),
        )
        .expect("gap build");
        let shifted = IndentedCodeLineV1::code(1, 6, 4, 1).expect("shifted");
        assert!(matches!(
            gap.offer_page(&[shifted]),
            Err(M11IndentedCodeProjectionError::CoverageMismatch)
        ));
        gap.begin_cancel(&mut runtime).expect("cancel gap");
        while !gap
            .poll_cancel(&mut runtime, 16)
            .expect("reclaim gap")
            .complete()
        {}
        drop(gap);

        let mut trailing = M11IndentedCodeProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("lease"),
            0..source.len(),
            0..source.len(),
            profile(1),
        )
        .expect("trailing build");
        let code = IndentedCodeLineV1::code(0, 6, 4, 1).expect("code");
        let blank = IndentedCodeLineV1::internal_blank(6, 1, 0).expect("blank");
        accept_page(&mut trailing, &mut runtime, &[code, blank]);
        assert!(matches!(
            trailing.finish_input(),
            Err(M11IndentedCodeProjectionError::TrailingBlankLine)
        ));
        trailing
            .begin_cancel(&mut runtime)
            .expect("cancel trailing");
        while !trailing
            .poll_cancel(&mut runtime, 16)
            .expect("reclaim trailing")
            .complete()
        {}
        drop(trailing);
        close(runtime);
    }

    #[test]
    fn cancellation_and_root_release_reclaim_every_arena_page() {
        let source = "    code\n";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let line = IndentedCodeLineV1::code(0, source.len() as u32, 4, 4).expect("line");
        let mut cancelled = M11IndentedCodeProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("lease"),
            0..source.len(),
            0..source.len(),
            profile(2),
        )
        .expect("cancel build");
        accept_page(&mut cancelled, &mut runtime, &[line]);
        cancelled.begin_cancel(&mut runtime).expect("begin cancel");
        while !cancelled
            .poll_cancel(&mut runtime, 1)
            .expect("poll cancel")
            .complete()
        {}
        drop(cancelled);

        let mut build = M11IndentedCodeProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("lease"),
            0..source.len(),
            0..source.len(),
            profile(2),
        )
        .expect("root build");
        accept_page(&mut build, &mut runtime, &[line]);
        let mut root = finish(&mut build, &mut runtime);
        release(&mut root, &mut runtime);
        drop(root);
        close(runtime);
    }

    #[test]
    fn cursor_rejects_wrong_authority_and_commitment() {
        let source = "    code\n";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let version = runtime.current_source_version().expect("source");
        let line = IndentedCodeLineV1::code(0, source.len() as u32, 4, 4).expect("line");
        let mut build = M11IndentedCodeProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("lease"),
            0..source.len(),
            0..source.len(),
            profile(3),
        )
        .expect("build");
        accept_page(&mut build, &mut runtime, &[line]);
        let mut root = finish(&mut build, &mut runtime);
        assert!(matches!(
            root.cursor(
                &runtime,
                version,
                profile(4),
                0..source.len() as u32,
                0..source.len() as u32,
            ),
            Err(M11IndentedCodeProjectionError::ParserProfileMismatch)
        ));
        root.descriptor.ordered_commitment256[0] ^= 1;
        let mut cursor = root
            .cursor(
                &runtime,
                version,
                profile(3),
                0..source.len() as u32,
                0..source.len() as u32,
            )
            .expect("cursor");
        loop {
            match cursor.poll(&runtime) {
                Ok(M11IndentedCodeProjectionCursorPoll::Pending { .. })
                | Ok(M11IndentedCodeProjectionCursorPoll::Line { .. }) => {}
                Err(M11IndentedCodeProjectionError::CommitmentMismatch) => break,
                other => panic!("unexpected cursor result {other:?}"),
            }
        }
        drop(cursor);
        release(&mut root, &mut runtime);
        drop(root);
        close(runtime);
    }

    #[test]
    fn requested_window_must_be_full_line_bounded_and_at_most_eight_kibibytes() {
        let source = format!("    {}\n", "x".repeat(INDENTED_CODE_WINDOW_MAX_BYTES));
        let runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        assert!(matches!(
            M11IndentedCodeProjectionBuild::new(
                &runtime,
                runtime.snapshot_current_source().expect("lease"),
                0..source.len(),
                0..source.len(),
                profile(5),
            ),
            Err(M11IndentedCodeProjectionError::WindowTooLarge { .. })
        ));
        assert!(matches!(
            M11IndentedCodeProjectionBuild::new(
                &runtime,
                runtime.snapshot_current_source().expect("lease"),
                0..source.len(),
                1..2,
                profile(5),
            ),
            Err(M11IndentedCodeProjectionError::InvalidAuthority(_))
        ));
        close(runtime);
    }
}
