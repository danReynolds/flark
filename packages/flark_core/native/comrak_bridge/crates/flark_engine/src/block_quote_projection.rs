//! Parser-owned persistent projection payload for depth-one block quotes.
//!
//! This schema records physical source geometry only. Every record covers one
//! complete physical line in an exact, at-most-8-KiB window of one immutable
//! block quote. Marked and lazy paragraph-continuation lines remain distinct;
//! consumers project them mechanically without reinterpreting Markdown.

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

const STREAM_TAG: u32 = u32::from_le_bytes(*b"BQP2");
const SCHEMA: u32 = 2;
const PAGE_MAGIC: [u8; 4] = *b"BQP2";
const PAGE_HEADER_BYTES: usize = 16;
/// Canonical byte width of one [`BlockQuoteLineV1`].
pub const BLOCK_QUOTE_LINE_V1_BYTES: usize = 28;
/// Maximum authoritative physical source window accepted by this urgent path.
pub const BLOCK_QUOTE_WINDOW_MAX_BYTES: usize = 8 * 1024;
/// Maximum line records in one parser-defined logical page.
pub const BLOCK_QUOTE_LINES_PER_PAGE_MAX: usize =
    (M11_PARSER_PAGE_MAX_RECORD_BYTES - PAGE_HEADER_BYTES) / BLOCK_QUOTE_LINE_V1_BYTES;

const COMMITMENT_DOMAIN: &[u8] = b"flark.marked-line-projection.v2\0";
const COMMITMENT_TRAILER: &[u8] = b"flark.marked-line-projection.end.v2\0";
const PERSISTENT_DESCRIPTOR_MAGIC: [u8; 4] = *b"BQR2";
const PERSISTENT_DESCRIPTOR_SCHEMA: u32 = 2;
pub(crate) const PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES: usize = 168;

/// Authenticated interpretation of the shared physical-line record stream.
///
/// Both projections need the same bounded persistent page mechanics, but the
/// final word in each 28-byte record has deliberately different semantics.
/// Carrying this discriminator in the descriptor and commitment prevents a
/// bullet-list payload from ever satisfying a block-quote query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11MarkedLineProjectionKind {
    BlockQuote = 0,
    BulletList = 1,
    OrderedList = 2,
}

impl M11MarkedLineProjectionKind {
    const fn from_wire(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::BlockQuote),
            1 => Some(Self::BulletList),
            2 => Some(Self::OrderedList),
            _ => None,
        }
    }
}

/// The line owns a source-backed depth-one block-quote marker and prefix.
pub const BLOCK_QUOTE_LINE_FLAG_MARKED: u32 = 1;
/// The line remains in the same child paragraph through CommonMark lazy
/// continuation and therefore owns no quote-marker prefix.
pub const BLOCK_QUOTE_LINE_FLAG_LAZY: u32 = 1 << 1;
const KNOWN_LINE_FLAGS: u32 = BLOCK_QUOTE_LINE_FLAG_MARKED | BLOCK_QUOTE_LINE_FLAG_LAZY;

/// One canonical physical-line projection record.
///
/// `relative_line_start` is relative to the descriptor's physical block
/// start. `hidden_prefix_length` is nonzero only for a marked line.
/// `content_length` names source-backed paragraph bytes; the physical EOL is
/// the remainder of `physical_source_length`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockQuoteLineV1 {
    relative_line_start: u32,
    physical_source_length: u32,
    hidden_prefix_length: u32,
    continuation_prefix_start: u32,
    continuation_prefix_end: u32,
    content_length: u32,
    flags: u32,
    projection_kind: M11MarkedLineProjectionKind,
}

impl BlockQuoteLineV1 {
    /// Creates and validates one top-level block-quote record.
    pub fn new(
        relative_line_start: u32,
        physical_source_length: u32,
        hidden_prefix_length: u32,
        content_length: u32,
        flags: u32,
    ) -> Result<Self, M11BlockQuoteProjectionError> {
        let line = Self {
            relative_line_start,
            physical_source_length,
            hidden_prefix_length,
            continuation_prefix_start: 0,
            continuation_prefix_end: 0,
            content_length,
            flags,
            projection_kind: M11MarkedLineProjectionKind::BlockQuote,
        };
        validate_line(line, M11MarkedLineProjectionKind::BlockQuote)?;
        Ok(line)
    }

    /// Convenience constructor for one source-marked quote line.
    pub fn marked(
        relative_line_start: u32,
        physical_source_length: u32,
        hidden_prefix_length: u32,
        content_length: u32,
    ) -> Result<Self, M11BlockQuoteProjectionError> {
        Self::new(
            relative_line_start,
            physical_source_length,
            hidden_prefix_length,
            content_length,
            BLOCK_QUOTE_LINE_FLAG_MARKED,
        )
    }

    /// Convenience constructor for one lazy paragraph-continuation line.
    pub fn lazy(
        relative_line_start: u32,
        physical_source_length: u32,
        content_length: u32,
    ) -> Result<Self, M11BlockQuoteProjectionError> {
        Self::new(
            relative_line_start,
            physical_source_length,
            0,
            content_length,
            BLOCK_QUOTE_LINE_FLAG_LAZY,
        )
    }

    /// Creates one certified tight bullet-list item record.
    ///
    /// The shared record's final word carries the visible content's UTF-16
    /// length for this authenticated stream kind. Marker spelling is
    /// homogeneous list-level structure and therefore is not repeated here.
    pub fn bullet_item(
        relative_line_start: u32,
        physical_source_length: u32,
        hidden_prefix_length: u32,
        continuation_prefix_start: u32,
        continuation_prefix_end: u32,
        content_length: u32,
        content_utf16_length: u32,
    ) -> Result<Self, M11BlockQuoteProjectionError> {
        let line = Self {
            relative_line_start,
            physical_source_length,
            hidden_prefix_length,
            continuation_prefix_start,
            continuation_prefix_end,
            content_length,
            flags: content_utf16_length,
            projection_kind: M11MarkedLineProjectionKind::BulletList,
        };
        validate_line(line, M11MarkedLineProjectionKind::BulletList)?;
        Ok(line)
    }

    /// Creates one certified tight ordered-list item record.
    ///
    /// The shared record carries only source/projection geometry. Ordered
    /// marker spelling and list-level start/delimiter metadata are
    /// authenticated by the ordered-list sidecar that owns this stream.
    pub fn ordered_item(
        relative_line_start: u32,
        physical_source_length: u32,
        hidden_prefix_length: u32,
        continuation_prefix_start: u32,
        continuation_prefix_end: u32,
        content_length: u32,
        content_utf16_length: u32,
    ) -> Result<Self, M11BlockQuoteProjectionError> {
        let line = Self {
            relative_line_start,
            physical_source_length,
            hidden_prefix_length,
            continuation_prefix_start,
            continuation_prefix_end,
            content_length,
            flags: content_utf16_length,
            projection_kind: M11MarkedLineProjectionKind::OrderedList,
        };
        validate_line(line, M11MarkedLineProjectionKind::OrderedList)?;
        Ok(line)
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

    /// Parser-certified source span to remove or reproduce when continuing
    /// this bullet item, relative to the physical line start.
    ///
    /// Block-quote records always return `0..0`.
    #[must_use]
    pub const fn continuation_prefix_start(self) -> u32 {
        self.continuation_prefix_start
    }

    #[must_use]
    pub const fn continuation_prefix_end(self) -> u32 {
        self.continuation_prefix_end
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
    pub const fn projection_kind(self) -> M11MarkedLineProjectionKind {
        self.projection_kind
    }

    /// Returns the bullet item's visible UTF-16 length.
    ///
    /// Callers must first authenticate that the enclosing descriptor is a
    /// [`M11MarkedLineProjectionKind::BulletList`] stream.
    #[must_use]
    pub const fn bullet_content_utf16_length(self) -> u32 {
        self.flags
    }

    /// Returns the ordered item's visible UTF-16 length.
    ///
    /// Callers must first authenticate that the enclosing descriptor is an
    /// [`M11MarkedLineProjectionKind::OrderedList`] stream.
    #[must_use]
    pub const fn ordered_content_utf16_length(self) -> u32 {
        self.flags
    }

    #[must_use]
    pub const fn is_marked(self) -> bool {
        self.flags == BLOCK_QUOTE_LINE_FLAG_MARKED
    }

    #[must_use]
    pub const fn is_lazy(self) -> bool {
        self.flags == BLOCK_QUOTE_LINE_FLAG_LAZY
    }

    #[must_use]
    pub const fn physical_eol_length(self) -> u32 {
        self.physical_source_length
            .saturating_sub(self.hidden_prefix_length)
            .saturating_sub(self.content_length)
    }

    pub fn relative_source_range(self) -> Result<Range<u32>, M11BlockQuoteProjectionError> {
        Ok(self.relative_line_start
            ..self
                .relative_line_start
                .checked_add(self.physical_source_length)
                .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?)
    }

    pub fn absolute_source_range(
        self,
        descriptor: &M11BlockQuoteProjectionDescriptor,
    ) -> Result<Range<u32>, M11BlockQuoteProjectionError> {
        let start = descriptor
            .physical_block_range
            .start
            .checked_add(self.relative_line_start)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        Ok(start
            ..start
                .checked_add(self.physical_source_length)
                .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?)
    }

    pub fn absolute_content_range(
        self,
        descriptor: &M11BlockQuoteProjectionDescriptor,
    ) -> Result<Range<u32>, M11BlockQuoteProjectionError> {
        let source = self.absolute_source_range(descriptor)?;
        let start = source
            .start
            .checked_add(self.hidden_prefix_length)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        Ok(start
            ..start
                .checked_add(self.content_length)
                .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?)
    }
}

/// Exact authority and authenticated summary of one projection window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11BlockQuoteProjectionDescriptor {
    source: SourceVersion,
    parser_profile: ParserProfileId,
    projection_kind: M11MarkedLineProjectionKind,
    physical_block_range: Range<u32>,
    requested_window: Range<u32>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    logical_page_count: u64,
    line_count: u64,
    storage_page_count: u64,
    storage_payload_bytes: u64,
    storage_encoded_bytes: u64,
    storage_checksum256: [u8; 32],
    ordered_commitment256: [u8; 32],
}

impl M11BlockQuoteProjectionDescriptor {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn parser_profile(&self) -> ParserProfileId {
        self.parser_profile
    }

    #[must_use]
    pub const fn projection_kind(&self) -> M11MarkedLineProjectionKind {
        self.projection_kind
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
    pub const fn projected_utf8_length(&self) -> u32 {
        self.projected_utf8_length
    }

    #[must_use]
    pub const fn projected_utf16_length(&self) -> u32 {
        self.projected_utf16_length
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
pub(crate) struct PersistentM11BlockQuoteProjectionDescriptor {
    source: SourceVersion,
    parser_profile: ParserProfileId,
    projection_kind: M11MarkedLineProjectionKind,
    block_start: u32,
    block_end: u32,
    window_start: u32,
    window_end: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    logical_page_count: u64,
    line_count: u64,
    storage_page_count: u64,
    payload_bytes: u64,
    encoded_bytes: u64,
    checksum: [u8; 32],
    ordered_commitment256: [u8; 32],
}

impl PersistentM11BlockQuoteProjectionDescriptor {
    pub(crate) const fn source(self) -> SourceVersion {
        self.source
    }

    pub(crate) const fn parser_profile(self) -> ParserProfileId {
        self.parser_profile
    }

    pub(crate) const fn projection_kind(self) -> M11MarkedLineProjectionKind {
        self.projection_kind
    }

    pub(crate) fn physical_block_range(self) -> Range<u32> {
        self.block_start..self.block_end
    }

    pub(crate) fn requested_window(self) -> Range<u32> {
        self.window_start..self.window_end
    }

    pub(crate) const fn projected_utf8_length(self) -> u32 {
        self.projected_utf8_length
    }

    pub(crate) const fn projected_utf16_length(self) -> u32 {
        self.projected_utf16_length
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

    fn public_descriptor(self) -> M11BlockQuoteProjectionDescriptor {
        M11BlockQuoteProjectionDescriptor {
            source: self.source,
            parser_profile: self.parser_profile,
            projection_kind: self.projection_kind,
            physical_block_range: self.physical_block_range(),
            requested_window: self.requested_window(),
            projected_utf8_length: self.projected_utf8_length,
            projected_utf16_length: self.projected_utf16_length,
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
pub enum M11BlockQuoteProjectionError {
    InvalidAuthority(&'static str),
    WindowTooLarge { bytes: usize, cap: usize },
    EmptyLogicalPage,
    TooManyLines { lines: usize, cap: usize },
    InvalidLine(&'static str),
    CoverageMismatch,
    ProjectedLengthMismatch,
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

impl fmt::Display for M11BlockQuoteProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAuthority(message) => {
                write!(formatter, "invalid block-quote authority: {message}")
            }
            Self::WindowTooLarge { bytes, cap } => write!(
                formatter,
                "block-quote window has {bytes} bytes above the {cap}-byte cap"
            ),
            Self::EmptyLogicalPage => {
                formatter.write_str("block-quote logical pages must not be empty")
            }
            Self::TooManyLines { lines, cap } => write!(
                formatter,
                "block-quote page has {lines} lines above the {cap}-line cap"
            ),
            Self::InvalidLine(message) => {
                write!(formatter, "invalid block-quote line: {message}")
            }
            Self::CoverageMismatch => formatter
                .write_str("block-quote lines do not exhaustively tile the requested window"),
            Self::ProjectedLengthMismatch => {
                formatter.write_str("block-quote projected lengths do not match their authority")
            }
            Self::CoordinateOverflow => formatter.write_str("block-quote coordinate overflow"),
            Self::SourceAuthorityMismatch => {
                formatter.write_str("block-quote source authority mismatch")
            }
            Self::ParserProfileMismatch => {
                formatter.write_str("block-quote parser profile mismatch")
            }
            Self::PhysicalBlockMismatch => {
                formatter.write_str("block-quote physical block authority mismatch")
            }
            Self::RequestedWindowMismatch => {
                formatter.write_str("block-quote requested window authority mismatch")
            }
            Self::InvalidState => {
                formatter.write_str("block-quote projection owner is in the wrong state")
            }
            Self::CommitmentMismatch => {
                formatter.write_str("block-quote ordered commitment mismatch")
            }
            Self::Malformed(message) => write!(
                formatter,
                "malformed block-quote projection page: {message}"
            ),
            Self::Pages(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11BlockQuoteProjectionError {}

impl From<M11ParserPageError> for M11BlockQuoteProjectionError {
    fn from(value: M11ParserPageError) -> Self {
        Self::Pages(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11BlockQuoteProjectionBuildStatus {
    NeedsPage,
    Pending,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11BlockQuoteProjectionBuildPoll {
    status: M11BlockQuoteProjectionBuildStatus,
    transitions: usize,
}

impl M11BlockQuoteProjectionBuildPoll {
    #[must_use]
    pub const fn status(self) -> M11BlockQuoteProjectionBuildStatus {
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

/// Move-only, fuelled builder for one exact top-level block-quote window.
#[must_use = "block-quote builds require root transfer or explicit cancellation"]
pub struct M11BlockQuoteProjectionBuild {
    inner: M11ParserPageBuild,
    source: SourceVersion,
    parser_profile: ParserProfileId,
    projection_kind: M11MarkedLineProjectionKind,
    physical_block_range: Range<u32>,
    requested_window: Range<u32>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    phase: BuildPhase,
    next_relative_start: u32,
    saw_eof_line: bool,
    saw_terminal_empty: bool,
    observed_projected_utf8_length: u32,
    observed_projected_utf16_length: u32,
    logical_page_count: u64,
    line_count: u64,
    stream_hasher: blake3::Hasher,
    ordered_commitment256: Option<[u8; 32]>,
    output: Option<M11BlockQuoteProjectionRoot>,
    failed_root: Option<M11ParserPageRoot>,
}

impl fmt::Debug for M11BlockQuoteProjectionBuild {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BlockQuoteProjectionBuild")
            .field("source", &self.source)
            .field("parser_profile", &self.parser_profile)
            .field("projection_kind", &self.projection_kind)
            .field("physical_block_range", &self.physical_block_range)
            .field("requested_window", &self.requested_window)
            .field("projected_utf8_length", &self.projected_utf8_length)
            .field("projected_utf16_length", &self.projected_utf16_length)
            .field("phase", &self.phase)
            .field("logical_page_count", &self.logical_page_count)
            .field("line_count", &self.line_count)
            .finish_non_exhaustive()
    }
}

impl M11BlockQuoteProjectionBuild {
    pub fn new(
        runtime: &DocumentRuntime,
        lease: SourceSnapshotLease,
        physical_block_range: Range<usize>,
        requested_window: Range<usize>,
        projected_utf8_length: u32,
        projected_utf16_length: u32,
        parser_profile: ParserProfileId,
    ) -> Result<Self, M11BlockQuoteProjectionError> {
        Self::new_with_kind(
            runtime,
            lease,
            physical_block_range,
            requested_window,
            projected_utf8_length,
            projected_utf16_length,
            parser_profile,
            M11MarkedLineProjectionKind::BlockQuote,
        )
    }

    /// Creates the shared persistent build with bullet-list record semantics.
    pub fn new_bullet_list(
        runtime: &DocumentRuntime,
        lease: SourceSnapshotLease,
        physical_block_range: Range<usize>,
        requested_window: Range<usize>,
        projected_utf8_length: u32,
        projected_utf16_length: u32,
        parser_profile: ParserProfileId,
    ) -> Result<Self, M11BlockQuoteProjectionError> {
        Self::new_with_kind(
            runtime,
            lease,
            physical_block_range,
            requested_window,
            projected_utf8_length,
            projected_utf16_length,
            parser_profile,
            M11MarkedLineProjectionKind::BulletList,
        )
    }

    /// Creates the shared persistent build with ordered-list record semantics.
    pub fn new_ordered_list(
        runtime: &DocumentRuntime,
        lease: SourceSnapshotLease,
        physical_block_range: Range<usize>,
        requested_window: Range<usize>,
        projected_utf8_length: u32,
        projected_utf16_length: u32,
        parser_profile: ParserProfileId,
    ) -> Result<Self, M11BlockQuoteProjectionError> {
        Self::new_with_kind(
            runtime,
            lease,
            physical_block_range,
            requested_window,
            projected_utf8_length,
            projected_utf16_length,
            parser_profile,
            M11MarkedLineProjectionKind::OrderedList,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_with_kind(
        runtime: &DocumentRuntime,
        lease: SourceSnapshotLease,
        physical_block_range: Range<usize>,
        requested_window: Range<usize>,
        projected_utf8_length: u32,
        projected_utf16_length: u32,
        parser_profile: ParserProfileId,
        projection_kind: M11MarkedLineProjectionKind,
    ) -> Result<Self, M11BlockQuoteProjectionError> {
        validate_authority_ranges(
            &lease,
            &physical_block_range,
            &requested_window,
            projected_utf8_length,
            projected_utf16_length,
            projection_kind,
        )?;
        let source = lease.version();
        let block = range_u32(&physical_block_range)?;
        let window = range_u32(&requested_window)?;
        let next_relative_start = window
            .start
            .checked_sub(block.start)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        let inner = M11ParserPageBuild::new(runtime, lease, physical_block_range, STREAM_TAG)?;
        Ok(Self {
            inner,
            source,
            parser_profile,
            projection_kind,
            physical_block_range: block.clone(),
            requested_window: window.clone(),
            projected_utf8_length,
            projected_utf16_length,
            phase: BuildPhase::Accepting,
            next_relative_start,
            saw_eof_line: false,
            saw_terminal_empty: false,
            observed_projected_utf8_length: 0,
            observed_projected_utf16_length: 0,
            logical_page_count: 0,
            line_count: 0,
            stream_hasher: begin_commitment(
                source,
                parser_profile,
                &block,
                &window,
                projected_utf8_length,
                projected_utf16_length,
                projection_kind,
            ),
            ordered_commitment256: None,
            output: None,
            failed_root: None,
        })
    }

    /// Offers one explicit logical page of source-ordered physical lines.
    pub fn offer_page(
        &mut self,
        lines: &[BlockQuoteLineV1],
    ) -> Result<(), M11BlockQuoteProjectionError> {
        if self.phase != BuildPhase::Accepting {
            return Err(M11BlockQuoteProjectionError::InvalidState);
        }
        if lines.is_empty() {
            return Err(M11BlockQuoteProjectionError::EmptyLogicalPage);
        }
        if lines.len() > BLOCK_QUOTE_LINES_PER_PAGE_MAX {
            return Err(M11BlockQuoteProjectionError::TooManyLines {
                lines: lines.len(),
                cap: BLOCK_QUOTE_LINES_PER_PAGE_MAX,
            });
        }

        let window_end_relative = self
            .requested_window
            .end
            .checked_sub(self.physical_block_range.start)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        let mut next = self.next_relative_start;
        let mut saw_eof = self.saw_eof_line;
        let mut saw_terminal_empty = self.saw_terminal_empty;
        let mut page_physical_bytes = 0_u32;
        let mut page_projected_bytes = 0_u32;
        let mut page_projected_utf16 = 0_u32;
        for line in lines {
            validate_line(*line, self.projection_kind)?;
            if line.relative_line_start != next || saw_eof || saw_terminal_empty {
                return Err(M11BlockQuoteProjectionError::CoverageMismatch);
            }
            let end = next
                .checked_add(line.physical_source_length)
                .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
            if end > window_end_relative {
                return Err(M11BlockQuoteProjectionError::CoverageMismatch);
            }
            if self.line_count == 0
                && page_physical_bytes == 0
                && !line_is_source_marked(*line, self.projection_kind)
            {
                return Err(M11BlockQuoteProjectionError::InvalidLine(
                    match self.projection_kind {
                        M11MarkedLineProjectionKind::BlockQuote => {
                            "the first block-quote line must be source-marked"
                        }
                        M11MarkedLineProjectionKind::BulletList => {
                            "the first bullet item must own a source marker"
                        }
                        M11MarkedLineProjectionKind::OrderedList => {
                            "the first ordered item must own a source marker"
                        }
                    },
                ));
            }
            if matches!(
                self.projection_kind,
                M11MarkedLineProjectionKind::BulletList | M11MarkedLineProjectionKind::OrderedList
            ) && line.content_length == 0
            {
                saw_terminal_empty = true;
            }
            if line.physical_eol_length() == 0 {
                let absolute_end = self
                    .physical_block_range
                    .start
                    .checked_add(end)
                    .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
                if absolute_end
                    != u32::try_from(self.source.byte_len())
                        .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?
                {
                    return Err(M11BlockQuoteProjectionError::InvalidLine(
                        "EOF ending does not terminate the immutable source",
                    ));
                }
                saw_eof = true;
            }
            next = end;
            page_physical_bytes = page_physical_bytes
                .checked_add(line.physical_source_length)
                .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
            page_projected_bytes = page_projected_bytes
                .checked_add(
                    line.content_length
                        .checked_add(line.physical_eol_length())
                        .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?,
                )
                .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
            if matches!(
                self.projection_kind,
                M11MarkedLineProjectionKind::BulletList | M11MarkedLineProjectionKind::OrderedList
            ) {
                page_projected_utf16 = page_projected_utf16
                    .checked_add(
                        line.flags()
                            .checked_add(line.physical_eol_length())
                            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?,
                    )
                    .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
            }
        }

        let encoded = encode_page(lines, page_physical_bytes)?;
        self.inner
            .offer_record(M11ParserPageRecord::new(encoded.as_bytes())?)?;
        append_page_to_commitment(&mut self.stream_hasher, encoded.as_bytes());
        self.next_relative_start = next;
        self.saw_eof_line = saw_eof;
        self.saw_terminal_empty = saw_terminal_empty;
        self.observed_projected_utf8_length = self
            .observed_projected_utf8_length
            .checked_add(page_projected_bytes)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        self.observed_projected_utf16_length = self
            .observed_projected_utf16_length
            .checked_add(page_projected_utf16)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        self.logical_page_count = self
            .logical_page_count
            .checked_add(1)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        self.line_count = self
            .line_count
            .checked_add(
                u64::try_from(lines.len())
                    .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?,
            )
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        Ok(())
    }

    pub fn finish_input(&mut self) -> Result<(), M11BlockQuoteProjectionError> {
        if self.phase != BuildPhase::Accepting {
            return Err(M11BlockQuoteProjectionError::InvalidState);
        }
        let expected_end = self
            .requested_window
            .end
            .checked_sub(self.physical_block_range.start)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        if self.line_count == 0 || self.next_relative_start != expected_end {
            return Err(M11BlockQuoteProjectionError::CoverageMismatch);
        }
        if self.observed_projected_utf8_length != self.projected_utf8_length {
            return Err(M11BlockQuoteProjectionError::ProjectedLengthMismatch);
        }
        if matches!(
            self.projection_kind,
            M11MarkedLineProjectionKind::BulletList | M11MarkedLineProjectionKind::OrderedList
        ) && self.observed_projected_utf16_length != self.projected_utf16_length
        {
            return Err(M11BlockQuoteProjectionError::ProjectedLengthMismatch);
        }
        self.inner.finish_input()?;
        self.ordered_commitment256 = Some(finish_commitment(
            &self.stream_hasher,
            self.logical_page_count,
            self.line_count,
        ));
        self.phase = BuildPhase::Finishing;
        Ok(())
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11BlockQuoteProjectionBuildPoll, M11BlockQuoteProjectionError> {
        if self.phase == BuildPhase::Failed {
            return Err(M11BlockQuoteProjectionError::InvalidState);
        }
        let poll = self.inner.poll(runtime, fuel)?;
        let status = match poll.status() {
            M11ParserPageBuildStatus::NeedsInput => {
                if self.phase != BuildPhase::Accepting {
                    self.phase = BuildPhase::Failed;
                    return Err(M11BlockQuoteProjectionError::InvalidState);
                }
                M11BlockQuoteProjectionBuildStatus::NeedsPage
            }
            M11ParserPageBuildStatus::Pending => M11BlockQuoteProjectionBuildStatus::Pending,
            M11ParserPageBuildStatus::Cancelled => {
                self.phase = BuildPhase::Cancelled;
                M11BlockQuoteProjectionBuildStatus::Cancelled
            }
            M11ParserPageBuildStatus::Complete => {
                if self.output.is_none() {
                    self.complete_root()?;
                }
                self.phase = BuildPhase::Complete;
                M11BlockQuoteProjectionBuildStatus::Complete
            }
        };
        Ok(M11BlockQuoteProjectionBuildPoll {
            status,
            transitions: poll.transitions(),
        })
    }

    fn complete_root(&mut self) -> Result<(), M11BlockQuoteProjectionError> {
        let commitment = self
            .ordered_commitment256
            .ok_or(M11BlockQuoteProjectionError::InvalidState)?;
        let root = self
            .inner
            .take_root()
            .ok_or(M11BlockQuoteProjectionError::InvalidState)?;
        let source_range = root.source_range();
        let exact = root.source() == self.source
            && source_range.start
                == usize::try_from(self.physical_block_range.start)
                    .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?
            && source_range.end
                == usize::try_from(self.physical_block_range.end)
                    .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?
            && root.stream_tag() == STREAM_TAG
            && root.record_count() == self.logical_page_count;
        if !exact {
            self.failed_root = Some(root);
            self.phase = BuildPhase::Failed;
            return Err(M11BlockQuoteProjectionError::Malformed(
                "generic page root changed typed authority",
            ));
        }
        let descriptor = M11BlockQuoteProjectionDescriptor {
            source: self.source,
            parser_profile: self.parser_profile,
            projection_kind: self.projection_kind,
            physical_block_range: self.physical_block_range.clone(),
            requested_window: self.requested_window.clone(),
            projected_utf8_length: self.projected_utf8_length,
            projected_utf16_length: self.projected_utf16_length,
            logical_page_count: self.logical_page_count,
            line_count: self.line_count,
            storage_page_count: root.page_count(),
            storage_payload_bytes: root.payload_bytes(),
            storage_encoded_bytes: root.encoded_bytes(),
            storage_checksum256: root.checksum(),
            ordered_commitment256: commitment,
        };
        self.output = Some(M11BlockQuoteProjectionRoot {
            inner: root,
            descriptor,
        });
        Ok(())
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11BlockQuoteProjectionError> {
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
    ) -> Result<M11ParserPageReclaimPoll, M11BlockQuoteProjectionError> {
        if self.phase != BuildPhase::Cancelled {
            return Err(M11BlockQuoteProjectionError::InvalidState);
        }
        let poll = self.inner.poll_cancel(runtime, fuel)?;
        if poll.complete() {
            self.output.take();
            self.failed_root.take();
        }
        Ok(poll)
    }

    #[must_use]
    pub fn take_root(&mut self) -> Option<M11BlockQuoteProjectionRoot> {
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

/// Move-only persistent root for one exact block-quote projection window.
#[must_use = "block-quote roots require transfer or explicit fuelled release"]
pub struct M11BlockQuoteProjectionRoot {
    inner: M11ParserPageRoot,
    descriptor: M11BlockQuoteProjectionDescriptor,
}

impl fmt::Debug for M11BlockQuoteProjectionRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BlockQuoteProjectionRoot")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl M11BlockQuoteProjectionRoot {
    #[must_use]
    pub const fn descriptor(&self) -> &M11BlockQuoteProjectionDescriptor {
        &self.descriptor
    }

    /// Returns the authority-free arena closure root and canonical 168-byte
    /// descriptor used by the independent host snapshot transport.
    pub(crate) fn transport_parts(
        &self,
        runtime: &DocumentRuntime,
        expected_source: SourceVersion,
        expected_profile: ParserProfileId,
    ) -> Result<
        (
            Option<ArenaId>,
            [u8; PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES],
        ),
        M11BlockQuoteProjectionError,
    > {
        if expected_source != self.descriptor.source {
            return Err(M11BlockQuoteProjectionError::SourceAuthorityMismatch);
        }
        if expected_profile != self.descriptor.parser_profile {
            return Err(M11BlockQuoteProjectionError::ParserProfileMismatch);
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
    ) -> Result<M11BlockQuoteProjectionCursor<'root>, M11BlockQuoteProjectionError> {
        if expected_source != self.descriptor.source {
            return Err(M11BlockQuoteProjectionError::SourceAuthorityMismatch);
        }
        if expected_profile != self.descriptor.parser_profile {
            return Err(M11BlockQuoteProjectionError::ParserProfileMismatch);
        }
        if expected_physical_block_range != self.descriptor.physical_block_range {
            return Err(M11BlockQuoteProjectionError::PhysicalBlockMismatch);
        }
        if expected_requested_window != self.descriptor.requested_window {
            return Err(M11BlockQuoteProjectionError::RequestedWindowMismatch);
        }
        let start = self
            .descriptor
            .requested_window
            .start
            .checked_sub(self.descriptor.physical_block_range.start)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        Ok(M11BlockQuoteProjectionCursor {
            inner: self.inner.cursor(runtime)?,
            descriptor: &self.descriptor,
            hasher: begin_commitment(
                self.descriptor.source,
                self.descriptor.parser_profile,
                &self.descriptor.physical_block_range,
                &self.descriptor.requested_window,
                self.descriptor.projected_utf8_length,
                self.descriptor.projected_utf16_length,
                self.descriptor.projection_kind,
            ),
            next_relative_start: start,
            saw_eof_line: false,
            saw_terminal_empty: false,
            observed_projected_utf8_length: 0,
            observed_projected_utf16_length: 0,
            observed_pages: 0,
            observed_lines: 0,
            current_page: None,
            complete: false,
        })
    }

    pub fn begin_release(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11BlockQuoteProjectionError> {
        self.inner.begin_release(runtime)?;
        Ok(())
    }

    pub fn poll_release(
        &self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ParserPageReclaimPoll, M11BlockQuoteProjectionError> {
        Ok(self.inner.poll_release(runtime, fuel)?)
    }
}

#[derive(Debug)]
pub enum M11BlockQuoteProjectionCursorPoll {
    Pending {
        transitions: usize,
    },
    Line {
        transitions: usize,
        line: BlockQuoteLineV1,
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
pub struct M11BlockQuoteProjectionCursor<'root> {
    inner: M11ParserPageCursor<'root>,
    descriptor: &'root M11BlockQuoteProjectionDescriptor,
    hasher: blake3::Hasher,
    next_relative_start: u32,
    saw_eof_line: bool,
    saw_terminal_empty: bool,
    observed_projected_utf8_length: u32,
    observed_projected_utf16_length: u32,
    observed_pages: u64,
    observed_lines: u64,
    current_page: Option<LoadedPage>,
    complete: bool,
}

impl M11BlockQuoteProjectionCursor<'_> {
    pub fn poll(
        &mut self,
        runtime: &DocumentRuntime,
    ) -> Result<M11BlockQuoteProjectionCursorPoll, M11BlockQuoteProjectionError> {
        if self.complete {
            return Ok(M11BlockQuoteProjectionCursorPoll::Complete { transitions: 0 });
        }
        if let Some(page) = self.current_page.as_mut() {
            if page.next_line < page.line_count {
                let line = decode_line(
                    page.record.as_bytes(),
                    page.next_line,
                    self.descriptor.projection_kind,
                )?;
                page.next_line += 1;
                return Ok(M11BlockQuoteProjectionCursorPoll::Line {
                    transitions: 1,
                    line,
                });
            }
            self.current_page = None;
        }

        match self.inner.poll(runtime)? {
            M11ParserPageCursorPoll::Pending { transitions } => {
                Ok(M11BlockQuoteProjectionCursorPoll::Pending { transitions })
            }
            M11ParserPageCursorPoll::Record {
                transitions,
                record,
            } => {
                let decoded = validate_page(
                    record.as_bytes(),
                    self.descriptor,
                    self.next_relative_start,
                    self.saw_eof_line,
                    self.saw_terminal_empty,
                    self.observed_lines,
                )?;
                append_page_to_commitment(&mut self.hasher, record.as_bytes());
                self.next_relative_start = decoded.next_relative_start;
                self.saw_eof_line = decoded.saw_eof_line;
                self.saw_terminal_empty = decoded.saw_terminal_empty;
                self.observed_projected_utf8_length = self
                    .observed_projected_utf8_length
                    .checked_add(decoded.projected_utf8_length)
                    .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
                self.observed_projected_utf16_length = self
                    .observed_projected_utf16_length
                    .checked_add(decoded.projected_utf16_length)
                    .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
                self.observed_pages = self
                    .observed_pages
                    .checked_add(1)
                    .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
                self.observed_lines = self
                    .observed_lines
                    .checked_add(
                        u64::try_from(decoded.line_count)
                            .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?,
                    )
                    .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
                let line = decode_line(record.as_bytes(), 0, self.descriptor.projection_kind)?;
                self.current_page = Some(LoadedPage {
                    record,
                    next_line: 1,
                    line_count: decoded.line_count,
                });
                Ok(M11BlockQuoteProjectionCursorPoll::Line { transitions, line })
            }
            M11ParserPageCursorPoll::Complete { transitions } => {
                validate_terminal_replay(
                    self.descriptor,
                    self.next_relative_start,
                    self.observed_pages,
                    self.observed_lines,
                    self.observed_projected_utf8_length,
                    self.observed_projected_utf16_length,
                )?;
                let actual =
                    finish_commitment(&self.hasher, self.observed_pages, self.observed_lines);
                if actual != self.descriptor.ordered_commitment256 {
                    return Err(M11BlockQuoteProjectionError::CommitmentMismatch);
                }
                self.complete = true;
                Ok(M11BlockQuoteProjectionCursorPoll::Complete { transitions })
            }
        }
    }

    #[must_use]
    pub const fn descriptor(&self) -> &M11BlockQuoteProjectionDescriptor {
        self.descriptor
    }
}

fn encode_persistent_descriptor(
    descriptor: &M11BlockQuoteProjectionDescriptor,
) -> Result<[u8; PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES], M11BlockQuoteProjectionError>
{
    let source_bytes = u32::try_from(descriptor.source.byte_len())
        .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?;
    let source_utf16 = u32::try_from(descriptor.source.utf16_len())
        .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?;
    let line_count = u32::try_from(descriptor.line_count)
        .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?;
    let mut output = [0_u8; PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES];
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
    write(&descriptor.projected_utf8_length.to_le_bytes());
    write(&descriptor.projected_utf16_length.to_le_bytes());
    write(&line_count.to_le_bytes());
    write(&(descriptor.projection_kind as u32).to_le_bytes());
    write(&descriptor.logical_page_count.to_le_bytes());
    write(&descriptor.storage_page_count.to_le_bytes());
    write(&descriptor.storage_payload_bytes.to_le_bytes());
    write(&descriptor.storage_encoded_bytes.to_le_bytes());
    write(&descriptor.storage_checksum256);
    write(&descriptor.ordered_commitment256);
    debug_assert_eq!(cursor, PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES);
    Ok(output)
}

pub(crate) fn decode_persistent_block_quote_projection_descriptor(
    bytes: &[u8],
    expected_source: SourceVersion,
    expected_profile: ParserProfileId,
) -> Result<PersistentM11BlockQuoteProjectionDescriptor, M11BlockQuoteProjectionError> {
    let expected_source_bytes = u32::try_from(expected_source.byte_len())
        .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?;
    let expected_source_utf16 = u32::try_from(expected_source.utf16_len())
        .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?;
    if bytes.len() != PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES
        || bytes[..4] != PERSISTENT_DESCRIPTOR_MAGIC
        || read_u32(bytes, 4)? != PERSISTENT_DESCRIPTOR_SCHEMA
        || read_u64(bytes, 8)? != expected_source.root().get()
        || read_u64(bytes, 16)? != expected_source.revision().get()
        || read_u32(bytes, 24)? != expected_source_bytes
        || read_u32(bytes, 28)? != expected_source_utf16
    {
        return Err(M11BlockQuoteProjectionError::SourceAuthorityMismatch);
    }
    if read_u64(bytes, 32)? != expected_profile.get() {
        return Err(M11BlockQuoteProjectionError::ParserProfileMismatch);
    }
    let block_start = read_u32(bytes, 40)?;
    let block_end = read_u32(bytes, 44)?;
    let window_start = read_u32(bytes, 48)?;
    let window_end = read_u32(bytes, 52)?;
    let projected_utf8_length = read_u32(bytes, 56)?;
    let projected_utf16_length = read_u32(bytes, 60)?;
    let line_count = u64::from(read_u32(bytes, 64)?);
    let projection_kind = M11MarkedLineProjectionKind::from_wire(read_u32(bytes, 68)?).ok_or(
        M11BlockQuoteProjectionError::Malformed("persistent descriptor line schema is unsupported"),
    )?;
    let logical_page_count = read_u64(bytes, 72)?;
    let storage_page_count = read_u64(bytes, 80)?;
    let payload_bytes = read_u64(bytes, 88)?;
    let encoded_bytes = read_u64(bytes, 96)?;
    let checksum: [u8; 32] = bytes[104..136]
        .try_into()
        .expect("fixed block-quote checksum");
    let ordered_commitment256: [u8; 32] = bytes[136..168]
        .try_into()
        .expect("fixed block-quote commitment");
    let window_bytes = window_end.checked_sub(window_start);
    if block_start >= block_end
        || window_start < block_start
        || window_start >= window_end
        || window_end > block_end
        || window_bytes.is_none_or(|value| value as usize > BLOCK_QUOTE_WINDOW_MAX_BYTES)
        || (projection_kind == M11MarkedLineProjectionKind::BlockQuote
            && (projected_utf8_length == 0 || projected_utf16_length == 0))
        || (projected_utf8_length == 0) != (projected_utf16_length == 0)
        || window_bytes.is_none_or(|value| projected_utf8_length > value)
        || usize::try_from(projected_utf16_length)
            .ok()
            .is_none_or(|length| length > expected_source.utf16_len())
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
        return Err(M11BlockQuoteProjectionError::Malformed(
            "persistent descriptor dimensions are invalid",
        ));
    }
    Ok(PersistentM11BlockQuoteProjectionDescriptor {
        source: expected_source,
        parser_profile: expected_profile,
        projection_kind,
        block_start,
        block_end,
        window_start,
        window_end,
        projected_utf8_length,
        projected_utf16_length,
        logical_page_count,
        line_count,
        storage_page_count,
        payload_bytes,
        encoded_bytes,
        checksum,
        ordered_commitment256,
    })
}

pub(crate) fn validate_persistent_block_quote_projection_root(
    arena: &PageArena,
    root: Option<ArenaId>,
    descriptor_bytes: &[u8],
    expected_source: SourceVersion,
    expected_profile: ParserProfileId,
) -> Result<PersistentM11BlockQuoteProjectionDescriptor, M11BlockQuoteProjectionError> {
    let descriptor = decode_persistent_block_quote_projection_descriptor(
        descriptor_bytes,
        expected_source,
        expected_profile,
    )?;
    validate_imported_m11_parser_page_root(arena, root, descriptor.page_claim())?;
    Ok(descriptor)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentM11BlockQuoteProjectionHostValidationPoll {
    pub(crate) transitions: usize,
    pub(crate) complete: bool,
}

/// Fuelled typed validation required before an imported closure is installed.
///
/// Generic admission authenticates the opaque page tree. This pass derives
/// line coverage and terminal semantics from every canonical `BQP2` record,
/// then authenticates the ordered BLAKE3 commitment.
pub(crate) struct PersistentM11BlockQuoteProjectionHostValidator {
    root: Option<ArenaId>,
    descriptor: PersistentM11BlockQuoteProjectionDescriptor,
    public_descriptor: M11BlockQuoteProjectionDescriptor,
    hasher: blake3::Hasher,
    next_relative_start: u32,
    saw_eof_line: bool,
    saw_terminal_empty: bool,
    observed_projected_utf8_length: u32,
    observed_projected_utf16_length: u32,
    observed_pages: u64,
    observed_lines: u64,
    complete: bool,
}

impl PersistentM11BlockQuoteProjectionHostValidator {
    pub(crate) fn new(
        arena: &PageArena,
        root: Option<ArenaId>,
        descriptor: PersistentM11BlockQuoteProjectionDescriptor,
    ) -> Result<Self, M11BlockQuoteProjectionError> {
        validate_imported_m11_parser_page_root(arena, root, descriptor.page_claim())?;
        let public_descriptor = descriptor.public_descriptor();
        let next_relative_start = descriptor
            .window_start
            .checked_sub(descriptor.block_start)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        Ok(Self {
            root,
            descriptor,
            public_descriptor,
            hasher: begin_commitment(
                descriptor.source,
                descriptor.parser_profile,
                &(descriptor.block_start..descriptor.block_end),
                &(descriptor.window_start..descriptor.window_end),
                descriptor.projected_utf8_length,
                descriptor.projected_utf16_length,
                descriptor.projection_kind,
            ),
            next_relative_start,
            saw_eof_line: false,
            saw_terminal_empty: false,
            observed_projected_utf8_length: 0,
            observed_projected_utf16_length: 0,
            observed_pages: 0,
            observed_lines: 0,
            complete: false,
        })
    }

    pub(crate) fn poll(
        &mut self,
        arena: &PageArena,
        fuel: usize,
    ) -> Result<PersistentM11BlockQuoteProjectionHostValidationPoll, M11BlockQuoteProjectionError>
    {
        if fuel == 0 {
            return Err(M11BlockQuoteProjectionError::Pages(
                M11ParserPageError::ZeroFuel,
            ));
        }
        if self.complete {
            return Ok(PersistentM11BlockQuoteProjectionHostValidationPoll {
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
                    self.saw_eof_line,
                    self.saw_terminal_empty,
                    self.observed_lines,
                )?;
                append_page_to_commitment(&mut self.hasher, record.as_bytes());
                self.next_relative_start = decoded.next_relative_start;
                self.saw_eof_line = decoded.saw_eof_line;
                self.saw_terminal_empty = decoded.saw_terminal_empty;
                self.observed_projected_utf8_length = self
                    .observed_projected_utf8_length
                    .checked_add(decoded.projected_utf8_length)
                    .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
                self.observed_projected_utf16_length = self
                    .observed_projected_utf16_length
                    .checked_add(decoded.projected_utf16_length)
                    .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
                self.observed_pages = self
                    .observed_pages
                    .checked_add(1)
                    .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
                self.observed_lines = self
                    .observed_lines
                    .checked_add(
                        u64::try_from(decoded.line_count)
                            .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?,
                    )
                    .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
                transitions += 1;
                continue;
            }
            validate_terminal_replay(
                &self.public_descriptor,
                self.next_relative_start,
                self.observed_pages,
                self.observed_lines,
                self.observed_projected_utf8_length,
                self.observed_projected_utf16_length,
            )?;
            let commitment =
                finish_commitment(&self.hasher, self.observed_pages, self.observed_lines);
            if commitment != self.descriptor.ordered_commitment256 {
                return Err(M11BlockQuoteProjectionError::CommitmentMismatch);
            }
            self.complete = true;
            transitions += 1;
            break;
        }
        Ok(PersistentM11BlockQuoteProjectionHostValidationPoll {
            transitions,
            complete: self.complete,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PersistentM11BlockQuoteProjectionHostCursorPoll {
    Line { line: BlockQuoteLineV1 },
    Complete,
}

/// Installed-only typed replay over an independently validated arena closure.
pub(crate) struct PersistentM11BlockQuoteProjectionHostCursor<'arena> {
    arena: &'arena PageArena,
    root: Option<ArenaId>,
    descriptor: PersistentM11BlockQuoteProjectionDescriptor,
    public_descriptor: M11BlockQuoteProjectionDescriptor,
    hasher: blake3::Hasher,
    next_page: u64,
    next_relative_start: u32,
    saw_eof_line: bool,
    saw_terminal_empty: bool,
    observed_projected_utf8_length: u32,
    observed_projected_utf16_length: u32,
    observed_lines: u64,
    current_page: Option<LoadedPage>,
    complete: bool,
}

impl<'arena> PersistentM11BlockQuoteProjectionHostCursor<'arena> {
    pub(crate) fn new(
        arena: &'arena PageArena,
        root: Option<ArenaId>,
        descriptor: PersistentM11BlockQuoteProjectionDescriptor,
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
                descriptor.projected_utf8_length,
                descriptor.projected_utf16_length,
                descriptor.projection_kind,
            ),
            next_page: 0,
            next_relative_start,
            saw_eof_line: false,
            saw_terminal_empty: false,
            observed_projected_utf8_length: 0,
            observed_projected_utf16_length: 0,
            observed_lines: 0,
            current_page: None,
            complete: false,
        }
    }

    pub(crate) fn poll(
        &mut self,
    ) -> Result<PersistentM11BlockQuoteProjectionHostCursorPoll, M11BlockQuoteProjectionError> {
        if self.complete {
            return Ok(PersistentM11BlockQuoteProjectionHostCursorPoll::Complete);
        }
        if let Some(page) = self.current_page.as_mut() {
            if page.next_line < page.line_count {
                let line = decode_line(
                    page.record.as_bytes(),
                    page.next_line,
                    self.descriptor.projection_kind,
                )?;
                page.next_line += 1;
                return Ok(PersistentM11BlockQuoteProjectionHostCursorPoll::Line { line });
            }
            self.current_page = None;
        }
        if self.next_page == self.descriptor.logical_page_count {
            validate_terminal_replay(
                &self.public_descriptor,
                self.next_relative_start,
                self.next_page,
                self.observed_lines,
                self.observed_projected_utf8_length,
                self.observed_projected_utf16_length,
            )?;
            let actual = finish_commitment(&self.hasher, self.next_page, self.observed_lines);
            if actual != self.descriptor.ordered_commitment256 {
                return Err(M11BlockQuoteProjectionError::CommitmentMismatch);
            }
            self.complete = true;
            return Ok(PersistentM11BlockQuoteProjectionHostCursorPoll::Complete);
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
            self.saw_eof_line,
            self.saw_terminal_empty,
            self.observed_lines,
        )?;
        append_page_to_commitment(&mut self.hasher, record.as_bytes());
        self.next_page = self
            .next_page
            .checked_add(1)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        self.next_relative_start = decoded.next_relative_start;
        self.saw_eof_line = decoded.saw_eof_line;
        self.saw_terminal_empty = decoded.saw_terminal_empty;
        self.observed_projected_utf8_length = self
            .observed_projected_utf8_length
            .checked_add(decoded.projected_utf8_length)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        self.observed_projected_utf16_length = self
            .observed_projected_utf16_length
            .checked_add(decoded.projected_utf16_length)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        self.observed_lines = self
            .observed_lines
            .checked_add(
                u64::try_from(decoded.line_count)
                    .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?,
            )
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        let line = decode_line(record.as_bytes(), 0, self.descriptor.projection_kind)?;
        self.current_page = Some(LoadedPage {
            record,
            next_line: 1,
            line_count: decoded.line_count,
        });
        Ok(PersistentM11BlockQuoteProjectionHostCursorPoll::Line { line })
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
    lines: &[BlockQuoteLineV1],
    page_physical_bytes: u32,
) -> Result<EncodedPage, M11BlockQuoteProjectionError> {
    let len = PAGE_HEADER_BYTES
        .checked_add(
            lines
                .len()
                .checked_mul(BLOCK_QUOTE_LINE_V1_BYTES)
                .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?,
        )
        .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
    if lines.is_empty()
        || lines.len() > BLOCK_QUOTE_LINES_PER_PAGE_MAX
        || len > M11_PARSER_PAGE_MAX_RECORD_BYTES
    {
        return Err(M11BlockQuoteProjectionError::Malformed(
            "logical page dimensions are invalid",
        ));
    }
    let mut bytes = [0_u8; M11_PARSER_PAGE_MAX_RECORD_BYTES];
    bytes[..4].copy_from_slice(&PAGE_MAGIC);
    bytes[4..8].copy_from_slice(&SCHEMA.to_le_bytes());
    bytes[8..10].copy_from_slice(
        &u16::try_from(lines.len())
            .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?
            .to_le_bytes(),
    );
    bytes[12..16].copy_from_slice(&page_physical_bytes.to_le_bytes());
    for (ordinal, line) in lines.iter().enumerate() {
        let start = PAGE_HEADER_BYTES + ordinal * BLOCK_QUOTE_LINE_V1_BYTES;
        bytes[start..start + 4].copy_from_slice(&line.relative_line_start.to_le_bytes());
        bytes[start + 4..start + 8].copy_from_slice(&line.physical_source_length.to_le_bytes());
        bytes[start + 8..start + 12].copy_from_slice(&line.hidden_prefix_length.to_le_bytes());
        bytes[start + 12..start + 16]
            .copy_from_slice(&line.continuation_prefix_start.to_le_bytes());
        bytes[start + 16..start + 20].copy_from_slice(&line.continuation_prefix_end.to_le_bytes());
        bytes[start + 20..start + 24].copy_from_slice(&line.content_length.to_le_bytes());
        bytes[start + 24..start + 28].copy_from_slice(&line.flags.to_le_bytes());
    }
    Ok(EncodedPage { bytes, len })
}

struct DecodedPage {
    line_count: usize,
    next_relative_start: u32,
    saw_eof_line: bool,
    saw_terminal_empty: bool,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
}

fn validate_page(
    bytes: &[u8],
    descriptor: &M11BlockQuoteProjectionDescriptor,
    expected_start: u32,
    previous_eof: bool,
    previous_terminal_empty: bool,
    observed_lines: u64,
) -> Result<DecodedPage, M11BlockQuoteProjectionError> {
    if bytes.get(..4) != Some(PAGE_MAGIC.as_slice()) || read_u32(bytes, 4)? != SCHEMA {
        return Err(M11BlockQuoteProjectionError::Malformed(
            "logical page magic or schema is unsupported",
        ));
    }
    let line_count = usize::from(read_u16(bytes, 8)?);
    if read_u16(bytes, 10)? != 0
        || line_count == 0
        || line_count > BLOCK_QUOTE_LINES_PER_PAGE_MAX
        || bytes.len() != PAGE_HEADER_BYTES + line_count * BLOCK_QUOTE_LINE_V1_BYTES
    {
        return Err(M11BlockQuoteProjectionError::Malformed(
            "logical page dimensions are invalid",
        ));
    }
    let expected_window_end = descriptor
        .requested_window
        .end
        .checked_sub(descriptor.physical_block_range.start)
        .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
    let mut next = expected_start;
    let mut saw_eof = previous_eof;
    let mut saw_terminal_empty = previous_terminal_empty;
    let mut physical_bytes = 0_u32;
    let mut projected_utf8_length = 0_u32;
    let mut projected_utf16_length = 0_u32;
    for ordinal in 0..line_count {
        let line = decode_line(bytes, ordinal, descriptor.projection_kind)?;
        validate_line(line, descriptor.projection_kind)?;
        if line.relative_line_start != next || saw_eof || saw_terminal_empty {
            return Err(M11BlockQuoteProjectionError::CoverageMismatch);
        }
        let end = next
            .checked_add(line.physical_source_length)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        if end > expected_window_end {
            return Err(M11BlockQuoteProjectionError::CoverageMismatch);
        }
        if observed_lines == 0
            && ordinal == 0
            && !line_is_source_marked(line, descriptor.projection_kind)
        {
            return Err(M11BlockQuoteProjectionError::InvalidLine(
                match descriptor.projection_kind {
                    M11MarkedLineProjectionKind::BlockQuote => {
                        "the first block-quote line must be source-marked"
                    }
                    M11MarkedLineProjectionKind::BulletList => {
                        "the first bullet item must own a source marker"
                    }
                    M11MarkedLineProjectionKind::OrderedList => {
                        "the first ordered item must own a source marker"
                    }
                },
            ));
        }
        if matches!(
            descriptor.projection_kind,
            M11MarkedLineProjectionKind::BulletList | M11MarkedLineProjectionKind::OrderedList
        ) && line.content_length == 0
        {
            saw_terminal_empty = true;
        }
        if line.physical_eol_length() == 0 {
            let absolute_end = descriptor
                .physical_block_range
                .start
                .checked_add(end)
                .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
            if absolute_end
                != u32::try_from(descriptor.source.byte_len())
                    .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?
            {
                return Err(M11BlockQuoteProjectionError::InvalidLine(
                    "EOF ending does not terminate the immutable source",
                ));
            }
            saw_eof = true;
        }
        next = end;
        physical_bytes = physical_bytes
            .checked_add(line.physical_source_length)
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        projected_utf8_length = projected_utf8_length
            .checked_add(
                line.content_length
                    .checked_add(line.physical_eol_length())
                    .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?,
            )
            .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        if matches!(
            descriptor.projection_kind,
            M11MarkedLineProjectionKind::BulletList | M11MarkedLineProjectionKind::OrderedList
        ) {
            projected_utf16_length = projected_utf16_length
                .checked_add(
                    line.flags()
                        .checked_add(line.physical_eol_length())
                        .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?,
                )
                .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
        }
    }
    if physical_bytes != read_u32(bytes, 12)? {
        return Err(M11BlockQuoteProjectionError::Malformed(
            "logical page physical byte total changed",
        ));
    }
    Ok(DecodedPage {
        line_count,
        next_relative_start: next,
        saw_eof_line: saw_eof,
        saw_terminal_empty,
        projected_utf8_length,
        projected_utf16_length,
    })
}

fn validate_terminal_replay(
    descriptor: &M11BlockQuoteProjectionDescriptor,
    next_relative_start: u32,
    observed_pages: u64,
    observed_lines: u64,
    observed_projected_utf8_length: u32,
    observed_projected_utf16_length: u32,
) -> Result<(), M11BlockQuoteProjectionError> {
    let expected_end = descriptor
        .requested_window
        .end
        .checked_sub(descriptor.physical_block_range.start)
        .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
    if next_relative_start != expected_end
        || observed_pages != descriptor.logical_page_count
        || observed_lines != descriptor.line_count
        || observed_lines == 0
    {
        return Err(M11BlockQuoteProjectionError::CoverageMismatch);
    }
    if observed_projected_utf8_length != descriptor.projected_utf8_length {
        return Err(M11BlockQuoteProjectionError::ProjectedLengthMismatch);
    }
    if matches!(
        descriptor.projection_kind,
        M11MarkedLineProjectionKind::BulletList | M11MarkedLineProjectionKind::OrderedList
    ) && observed_projected_utf16_length != descriptor.projected_utf16_length
    {
        return Err(M11BlockQuoteProjectionError::ProjectedLengthMismatch);
    }
    Ok(())
}

fn decode_line(
    bytes: &[u8],
    ordinal: usize,
    projection_kind: M11MarkedLineProjectionKind,
) -> Result<BlockQuoteLineV1, M11BlockQuoteProjectionError> {
    let start = PAGE_HEADER_BYTES
        .checked_add(
            ordinal
                .checked_mul(BLOCK_QUOTE_LINE_V1_BYTES)
                .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?,
        )
        .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
    let record = bytes.get(start..start + BLOCK_QUOTE_LINE_V1_BYTES).ok_or(
        M11BlockQuoteProjectionError::Malformed("line record is truncated"),
    )?;
    Ok(BlockQuoteLineV1 {
        relative_line_start: read_u32(record, 0)?,
        physical_source_length: read_u32(record, 4)?,
        hidden_prefix_length: read_u32(record, 8)?,
        continuation_prefix_start: read_u32(record, 12)?,
        continuation_prefix_end: read_u32(record, 16)?,
        content_length: read_u32(record, 20)?,
        flags: read_u32(record, 24)?,
        projection_kind,
    })
}

fn validate_line(
    line: BlockQuoteLineV1,
    projection_kind: M11MarkedLineProjectionKind,
) -> Result<(), M11BlockQuoteProjectionError> {
    if line.projection_kind != projection_kind {
        return Err(M11BlockQuoteProjectionError::InvalidLine(
            "line record kind does not match its authenticated stream",
        ));
    }
    if line.physical_source_length == 0 {
        return Err(M11BlockQuoteProjectionError::InvalidLine(
            "physical source length must be nonzero",
        ));
    }
    let prefix_and_content = line
        .hidden_prefix_length
        .checked_add(line.content_length)
        .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
    if prefix_and_content > line.physical_source_length {
        return Err(M11BlockQuoteProjectionError::InvalidLine(
            "hidden prefix and content exceed physical source length",
        ));
    }
    let eol = line.physical_source_length - prefix_and_content;
    if eol > 2 {
        return Err(M11BlockQuoteProjectionError::InvalidLine(
            "physical EOL length must be zero, one, or two bytes",
        ));
    }
    match projection_kind {
        M11MarkedLineProjectionKind::BlockQuote => {
            if line.continuation_prefix_start != 0
                || line.continuation_prefix_end != 0
                || line.flags & !KNOWN_LINE_FLAGS != 0
                || !matches!(
                    line.flags,
                    BLOCK_QUOTE_LINE_FLAG_MARKED | BLOCK_QUOTE_LINE_FLAG_LAZY
                )
            {
                return Err(M11BlockQuoteProjectionError::InvalidLine(
                    "exactly one marked/lazy flag is required",
                ));
            }
            if line.content_length == 0 {
                return Err(M11BlockQuoteProjectionError::InvalidLine(
                    "paragraph lines require source-backed content bytes",
                ));
            }
            if line.is_marked() && line.hidden_prefix_length == 0 {
                return Err(M11BlockQuoteProjectionError::InvalidLine(
                    "marked lines require a hidden quote prefix",
                ));
            }
            if line.is_lazy() && line.hidden_prefix_length != 0 {
                return Err(M11BlockQuoteProjectionError::InvalidLine(
                    "lazy lines cannot hide a quote prefix",
                ));
            }
        }
        M11MarkedLineProjectionKind::BulletList => {
            if line.hidden_prefix_length == 0 {
                return Err(M11BlockQuoteProjectionError::InvalidLine(
                    "bullet items require a hidden source prefix",
                ));
            }
            if (line.content_length == 0) != (line.flags == 0) || line.flags > line.content_length {
                return Err(M11BlockQuoteProjectionError::InvalidLine(
                    "bullet item UTF-8 and UTF-16 content lengths disagree",
                ));
            }
            if line.continuation_prefix_start >= line.continuation_prefix_end
                || line.continuation_prefix_end > line.hidden_prefix_length
            {
                return Err(M11BlockQuoteProjectionError::InvalidLine(
                    "bullet continuation prefix must be a nonempty hidden-prefix subrange",
                ));
            }
        }
        M11MarkedLineProjectionKind::OrderedList => {
            if line.hidden_prefix_length == 0 {
                return Err(M11BlockQuoteProjectionError::InvalidLine(
                    "ordered items require a hidden source prefix",
                ));
            }
            if (line.content_length == 0) != (line.flags == 0) || line.flags > line.content_length {
                return Err(M11BlockQuoteProjectionError::InvalidLine(
                    "ordered item UTF-8 and UTF-16 content lengths disagree",
                ));
            }
            if line.continuation_prefix_start >= line.continuation_prefix_end
                || line.continuation_prefix_end > line.hidden_prefix_length
            {
                return Err(M11BlockQuoteProjectionError::InvalidLine(
                    "ordered continuation prefix must be a nonempty hidden-prefix subrange",
                ));
            }
        }
    }
    line.relative_line_start
        .checked_add(line.physical_source_length)
        .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
    Ok(())
}

const fn line_is_source_marked(
    line: BlockQuoteLineV1,
    projection_kind: M11MarkedLineProjectionKind,
) -> bool {
    match projection_kind {
        M11MarkedLineProjectionKind::BlockQuote => line.is_marked(),
        M11MarkedLineProjectionKind::BulletList | M11MarkedLineProjectionKind::OrderedList => {
            line.hidden_prefix_length != 0
        }
    }
}

fn validate_authority_ranges(
    lease: &SourceSnapshotLease,
    block: &Range<usize>,
    window: &Range<usize>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    projection_kind: M11MarkedLineProjectionKind,
) -> Result<(), M11BlockQuoteProjectionError> {
    if block.start >= block.end
        || block.end > lease.version().byte_len()
        || window.start >= window.end
        || window.start < block.start
        || window.end > block.end
    {
        return Err(M11BlockQuoteProjectionError::InvalidAuthority(
            "block/window ranges are empty, reversed, outside source, or not nested",
        ));
    }
    let bytes = window.end - window.start;
    if bytes > BLOCK_QUOTE_WINDOW_MAX_BYTES {
        return Err(M11BlockQuoteProjectionError::WindowTooLarge {
            bytes,
            cap: BLOCK_QUOTE_WINDOW_MAX_BYTES,
        });
    }
    let window_start_utf16 = lease.utf16_offset_for_byte(window.start).map_err(|_| {
        M11BlockQuoteProjectionError::InvalidAuthority(
            "projection window start is not scalar-aligned",
        )
    })?;
    let window_end_utf16 = lease.utf16_offset_for_byte(window.end).map_err(|_| {
        M11BlockQuoteProjectionError::InvalidAuthority(
            "projection window end is not scalar-aligned",
        )
    })?;
    let window_utf16 = window_end_utf16
        .checked_sub(window_start_utf16)
        .ok_or(M11BlockQuoteProjectionError::CoordinateOverflow)?;
    if (projection_kind == M11MarkedLineProjectionKind::BlockQuote
        && (projected_utf8_length == 0 || projected_utf16_length == 0))
        || (projected_utf8_length == 0) != (projected_utf16_length == 0)
        || usize::try_from(projected_utf8_length)
            .ok()
            .is_none_or(|length| length > bytes)
        || usize::try_from(projected_utf16_length)
            .ok()
            .is_none_or(|length| length > window_utf16)
    {
        return Err(M11BlockQuoteProjectionError::ProjectedLengthMismatch);
    }
    for boundary in [block.start, window.start] {
        let scalar = lease.utf16_offset_for_byte(boundary).is_ok();
        let line = lease.is_physical_line_start(boundary).unwrap_or(false);
        if !scalar || !line {
            return Err(M11BlockQuoteProjectionError::InvalidAuthority(
                "block/window cuts must be complete physical-line boundaries",
            ));
        }
    }
    for boundary in [block.end, window.end] {
        let scalar = lease.utf16_offset_for_byte(boundary).is_ok();
        let line = boundary == lease.version().byte_len()
            || lease.is_physical_line_start(boundary).unwrap_or(false);
        if !scalar || !line {
            return Err(M11BlockQuoteProjectionError::InvalidAuthority(
                "block/window cuts must be complete physical-line boundaries",
            ));
        }
    }
    if u32::try_from(lease.version().byte_len()).is_err() {
        return Err(M11BlockQuoteProjectionError::CoordinateOverflow);
    }
    Ok(())
}

fn range_u32(range: &Range<usize>) -> Result<Range<u32>, M11BlockQuoteProjectionError> {
    Ok(
        u32::try_from(range.start).map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?
            ..u32::try_from(range.end)
                .map_err(|_| M11BlockQuoteProjectionError::CoordinateOverflow)?,
    )
}

fn begin_commitment(
    source: SourceVersion,
    parser_profile: ParserProfileId,
    block: &Range<u32>,
    window: &Range<u32>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    projection_kind: M11MarkedLineProjectionKind,
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
    hasher.update(&projected_utf8_length.to_le_bytes());
    hasher.update(&projected_utf16_length.to_le_bytes());
    hasher.update(&(projection_kind as u32).to_le_bytes());
    hasher
}

fn append_page_to_commitment(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn finish_commitment(hasher: &blake3::Hasher, pages: u64, lines: u64) -> [u8; 32] {
    let mut hasher = hasher.clone();
    hasher.update(COMMITMENT_TRAILER);
    hasher.update(&pages.to_le_bytes());
    hasher.update(&lines.to_le_bytes());
    *hasher.finalize().as_bytes()
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, M11BlockQuoteProjectionError> {
    let bytes = bytes
        .get(offset..offset + 2)
        .ok_or(M11BlockQuoteProjectionError::Malformed("u16 is truncated"))?;
    Ok(u16::from_le_bytes(
        bytes.try_into().expect("checked u16 width"),
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, M11BlockQuoteProjectionError> {
    let bytes = bytes
        .get(offset..offset + 4)
        .ok_or(M11BlockQuoteProjectionError::Malformed("u32 is truncated"))?;
    Ok(u32::from_le_bytes(
        bytes.try_into().expect("checked u32 width"),
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, M11BlockQuoteProjectionError> {
    let bytes = bytes
        .get(offset..offset + 8)
        .ok_or(M11BlockQuoteProjectionError::Malformed("u64 is truncated"))?;
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

    fn lines() -> [BlockQuoteLineV1; 3] {
        [
            BlockQuoteLineV1::marked(0, 9, 2, 5).expect("marked alpha"),
            BlockQuoteLineV1::lazy(9, 5, 4).expect("lazy continuation"),
            BlockQuoteLineV1::marked(14, 6, 2, 4).expect("marked beta"),
        ]
    }

    fn build(
        runtime: &DocumentRuntime,
        projected_utf8_length: u32,
        projected_utf16_length: u32,
    ) -> M11BlockQuoteProjectionBuild {
        let source = "> alpha\r\nlazy\n> beta";
        M11BlockQuoteProjectionBuild::new(
            runtime,
            runtime.snapshot_current_source().expect("lease"),
            0..source.len(),
            0..source.len(),
            projected_utf8_length,
            projected_utf16_length,
            profile(7),
        )
        .expect("build")
    }

    fn accept_page(
        build: &mut M11BlockQuoteProjectionBuild,
        runtime: &mut DocumentRuntime,
        lines: &[BlockQuoteLineV1],
    ) {
        build.offer_page(lines).expect("offer page");
        loop {
            match build.poll(runtime, 16).expect("poll page").status() {
                M11BlockQuoteProjectionBuildStatus::NeedsPage => break,
                M11BlockQuoteProjectionBuildStatus::Pending => {}
                other => panic!("unexpected build state {other:?}"),
            }
        }
    }

    fn finish(
        build: &mut M11BlockQuoteProjectionBuild,
        runtime: &mut DocumentRuntime,
    ) -> M11BlockQuoteProjectionRoot {
        build.finish_input().expect("finish input");
        loop {
            match build.poll(runtime, 16).expect("poll finish").status() {
                M11BlockQuoteProjectionBuildStatus::Pending => {}
                M11BlockQuoteProjectionBuildStatus::Complete => {
                    return build.take_root().expect("root");
                }
                other => panic!("unexpected finish state {other:?}"),
            }
        }
    }

    fn release(root: &mut M11BlockQuoteProjectionRoot, runtime: &mut DocumentRuntime) {
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

    #[test]
    fn marked_and_lazy_lines_round_trip_with_exact_authority_and_commitment() {
        let source = "> alpha\r\nlazy\n> beta";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let version = runtime.current_source_version().expect("source");
        let expected = lines();
        let mut build = build(&runtime, 16, 16);
        accept_page(&mut build, &mut runtime, &expected[..2]);
        accept_page(&mut build, &mut runtime, &expected[2..]);
        let mut root = finish(&mut build, &mut runtime);
        assert_eq!(root.descriptor().line_count(), 3);
        assert_eq!(root.descriptor().logical_page_count(), 2);
        assert_eq!(root.descriptor().projected_utf8_length(), 16);
        assert_eq!(root.descriptor().projected_utf16_length(), 16);
        assert_ne!(root.descriptor().ordered_commitment256(), [0; 32]);

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
                M11BlockQuoteProjectionCursorPoll::Pending { .. } => {}
                M11BlockQuoteProjectionCursorPoll::Line { line, .. } => replay.push(line),
                M11BlockQuoteProjectionCursorPoll::Complete { .. } => break,
            }
        }
        assert_eq!(replay, expected);
        drop(cursor);
        release(&mut root, &mut runtime);
        drop(root);
        close(runtime);
    }

    #[test]
    fn persistent_host_validation_and_cursor_replay_typed_lines() {
        let source = "> alpha\r\nlazy\n> beta";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let version = runtime.current_source_version().expect("source");
        let expected = lines();
        let mut build = build(&runtime, 16, 16);
        accept_page(&mut build, &mut runtime, &expected);
        let mut root = finish(&mut build, &mut runtime);
        let (arena_root, descriptor_bytes) = root
            .transport_parts(&runtime, version, profile(7))
            .expect("transport parts");
        assert_eq!(
            descriptor_bytes.len(),
            PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES
        );
        let descriptor = validate_persistent_block_quote_projection_root(
            runtime.producer_arena(),
            arena_root,
            &descriptor_bytes,
            version,
            profile(7),
        )
        .expect("typed imported root");
        assert_eq!(descriptor.line_count(), 3);
        assert_eq!(descriptor.projected_utf8_length(), 16);
        assert_eq!(descriptor.projected_utf16_length(), 16);

        let mut validator = PersistentM11BlockQuoteProjectionHostValidator::new(
            runtime.producer_arena(),
            arena_root,
            descriptor,
        )
        .expect("validator");
        loop {
            let poll = validator
                .poll(runtime.producer_arena(), 1)
                .expect("validation poll");
            assert!(poll.transitions <= 1);
            if poll.complete {
                break;
            }
        }
        drop(validator);

        let mut cursor = PersistentM11BlockQuoteProjectionHostCursor::new(
            runtime.producer_arena(),
            arena_root,
            descriptor,
        );
        let mut replay = Vec::new();
        while let PersistentM11BlockQuoteProjectionHostCursorPoll::Line { line } =
            cursor.poll().expect("host cursor")
        {
            replay.push(line);
        }
        assert_eq!(replay, expected);
        drop(cursor);
        release(&mut root, &mut runtime);
        drop(root);
        close(runtime);
    }

    #[test]
    fn line_schema_keeps_marked_and_lazy_semantics_disjoint() {
        assert!(BlockQuoteLineV1::marked(0, 8, 2, 5).is_ok());
        assert!(BlockQuoteLineV1::lazy(8, 5, 4).is_ok());
        for invalid in [
            BlockQuoteLineV1::new(0, 8, 2, 5, 0),
            BlockQuoteLineV1::new(
                0,
                8,
                2,
                5,
                BLOCK_QUOTE_LINE_FLAG_MARKED | BLOCK_QUOTE_LINE_FLAG_LAZY,
            ),
            BlockQuoteLineV1::new(0, 8, 0, 7, BLOCK_QUOTE_LINE_FLAG_MARKED),
            BlockQuoteLineV1::new(0, 8, 2, 5, BLOCK_QUOTE_LINE_FLAG_LAZY),
            BlockQuoteLineV1::new(0, 4, 2, 0, BLOCK_QUOTE_LINE_FLAG_MARKED),
        ] {
            assert!(matches!(
                invalid,
                Err(M11BlockQuoteProjectionError::InvalidLine(_))
            ));
        }
    }

    #[test]
    fn builder_rejects_lazy_root_gaps_and_wrong_projected_length() {
        let source = "> alpha\r\nlazy\n> beta";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");

        let mut lazy_root = build(&runtime, 16, 16);
        assert!(matches!(
            lazy_root.offer_page(&[BlockQuoteLineV1::lazy(0, 5, 4).expect("lazy")]),
            Err(M11BlockQuoteProjectionError::InvalidLine(
                "the first block-quote line must be source-marked"
            ))
        ));
        lazy_root.begin_cancel(&mut runtime).expect("cancel lazy");
        while !lazy_root
            .poll_cancel(&mut runtime, 16)
            .expect("poll cancel")
            .complete()
        {}
        drop(lazy_root);

        let mut gap = build(&runtime, 16, 16);
        let shifted = BlockQuoteLineV1::marked(1, 9, 2, 5).expect("shifted");
        assert!(matches!(
            gap.offer_page(&[shifted]),
            Err(M11BlockQuoteProjectionError::CoverageMismatch)
        ));
        gap.begin_cancel(&mut runtime).expect("cancel gap");
        while !gap
            .poll_cancel(&mut runtime, 16)
            .expect("poll cancel")
            .complete()
        {}
        drop(gap);

        let mut wrong_length = build(&runtime, 15, 16);
        accept_page(&mut wrong_length, &mut runtime, &lines());
        assert!(matches!(
            wrong_length.finish_input(),
            Err(M11BlockQuoteProjectionError::ProjectedLengthMismatch)
        ));
        wrong_length
            .begin_cancel(&mut runtime)
            .expect("cancel wrong length");
        while !wrong_length
            .poll_cancel(&mut runtime, 16)
            .expect("poll cancel")
            .complete()
        {}
        drop(wrong_length);
        close(runtime);
    }

    #[test]
    fn cancellation_and_release_reclaim_every_arena_page() {
        let source = "> alpha\r\nlazy\n> beta";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let expected = lines();

        let mut cancelled = build(&runtime, 16, 16);
        accept_page(&mut cancelled, &mut runtime, &expected);
        cancelled.begin_cancel(&mut runtime).expect("begin cancel");
        while !cancelled
            .poll_cancel(&mut runtime, 1)
            .expect("poll cancel")
            .complete()
        {}
        drop(cancelled);

        let mut build = build(&runtime, 16, 16);
        accept_page(&mut build, &mut runtime, &expected);
        let mut root = finish(&mut build, &mut runtime);
        release(&mut root, &mut runtime);
        drop(root);
        close(runtime);
    }

    #[test]
    fn bullet_stream_kind_is_disjoint_and_accepts_a_terminal_empty_projection() {
        let source = "- ";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let item = BlockQuoteLineV1::bullet_item(0, 2, 2, 0, 2, 0, 0).expect("empty item");

        let mut quote = M11BlockQuoteProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("quote lease"),
            0..source.len(),
            0..source.len(),
            1,
            1,
            profile(9),
        )
        .expect("quote build");
        assert!(matches!(
            quote.offer_page(&[item]),
            Err(M11BlockQuoteProjectionError::InvalidLine(
                "line record kind does not match its authenticated stream"
            ))
        ));
        quote.begin_cancel(&mut runtime).expect("cancel quote");
        while !quote
            .poll_cancel(&mut runtime, 16)
            .expect("poll quote cancel")
            .complete()
        {}
        drop(quote);

        let mut build = M11BlockQuoteProjectionBuild::new_bullet_list(
            &runtime,
            runtime.snapshot_current_source().expect("list lease"),
            0..source.len(),
            0..source.len(),
            0,
            0,
            profile(9),
        )
        .expect("list build");
        accept_page(&mut build, &mut runtime, &[item]);
        let mut root = finish(&mut build, &mut runtime);
        assert_eq!(
            root.descriptor().projection_kind(),
            M11MarkedLineProjectionKind::BulletList
        );
        assert_eq!(root.descriptor().projected_utf8_length(), 0);
        assert_eq!(root.descriptor().projected_utf16_length(), 0);

        let version = runtime.current_source_version().expect("source");
        let (arena_root, descriptor_bytes) = root
            .transport_parts(&runtime, version, profile(9))
            .expect("transport");
        let descriptor = validate_persistent_block_quote_projection_root(
            runtime.producer_arena(),
            arena_root,
            &descriptor_bytes,
            version,
            profile(9),
        )
        .expect("persistent descriptor");
        assert_eq!(
            descriptor.projection_kind(),
            M11MarkedLineProjectionKind::BulletList
        );
        let mut validator = PersistentM11BlockQuoteProjectionHostValidator::new(
            runtime.producer_arena(),
            arena_root,
            descriptor,
        )
        .expect("validator");
        while !validator
            .poll(runtime.producer_arena(), 1)
            .expect("validation")
            .complete
        {}
        drop(validator);
        release(&mut root, &mut runtime);
        drop(root);
        close(runtime);
    }

    #[test]
    fn bullet_stream_validates_unicode_utf16_and_terminal_empty_aggregates() {
        let source = "- α\n-   ";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let items = [
            BlockQuoteLineV1::bullet_item(0, 5, 2, 0, 2, 2, 1).expect("unicode item"),
            BlockQuoteLineV1::bullet_item(5, 4, 4, 0, 2, 0, 0).expect("terminal padded empty"),
        ];
        let mut build = M11BlockQuoteProjectionBuild::new_bullet_list(
            &runtime,
            runtime.snapshot_current_source().expect("lease"),
            0..source.len(),
            0..source.len(),
            3,
            2,
            profile(11),
        )
        .expect("build");
        accept_page(&mut build, &mut runtime, &items);
        let mut root = finish(&mut build, &mut runtime);
        assert_eq!(root.descriptor().projected_utf8_length(), 3);
        assert_eq!(root.descriptor().projected_utf16_length(), 2);
        let mut cursor = root
            .cursor(
                &runtime,
                runtime.current_source_version().expect("source"),
                profile(11),
                0..source.len() as u32,
                0..source.len() as u32,
            )
            .expect("cursor");
        let mut replay = Vec::new();
        loop {
            match cursor.poll(&runtime).expect("poll") {
                M11BlockQuoteProjectionCursorPoll::Pending { .. } => {}
                M11BlockQuoteProjectionCursorPoll::Line { line, .. } => replay.push(line),
                M11BlockQuoteProjectionCursorPoll::Complete { .. } => break,
            }
        }
        assert_eq!(replay, items);
        drop(cursor);
        release(&mut root, &mut runtime);
        drop(root);
        close(runtime);
    }

    #[test]
    fn ordered_stream_is_authenticated_disjoint_and_round_trips_shared_geometry() {
        let source = "1. α\n2. ";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let ordered_items = [
            BlockQuoteLineV1::ordered_item(0, 6, 3, 0, 3, 2, 1).expect("Unicode item"),
            BlockQuoteLineV1::ordered_item(6, 3, 3, 0, 3, 0, 0).expect("terminal empty"),
        ];
        let bullet_items = [
            BlockQuoteLineV1::bullet_item(0, 6, 3, 0, 3, 2, 1).expect("bullet geometry"),
            BlockQuoteLineV1::bullet_item(6, 3, 3, 0, 3, 0, 0).expect("bullet terminal empty"),
        ];
        assert_eq!(ordered_items[0].ordered_content_utf16_length(), 1);
        assert_eq!(bullet_items[0].bullet_content_utf16_length(), 1);

        let mut bullet = M11BlockQuoteProjectionBuild::new_bullet_list(
            &runtime,
            runtime.snapshot_current_source().expect("bullet lease"),
            0..source.len(),
            0..source.len(),
            3,
            2,
            profile(13),
        )
        .expect("bullet build");
        assert!(matches!(
            bullet.offer_page(&ordered_items),
            Err(M11BlockQuoteProjectionError::InvalidLine(
                "line record kind does not match its authenticated stream"
            ))
        ));
        accept_page(&mut bullet, &mut runtime, &bullet_items);
        let mut bullet_root = finish(&mut bullet, &mut runtime);

        let mut ordered = M11BlockQuoteProjectionBuild::new_ordered_list(
            &runtime,
            runtime.snapshot_current_source().expect("ordered lease"),
            0..source.len(),
            0..source.len(),
            3,
            2,
            profile(13),
        )
        .expect("ordered build");
        assert!(matches!(
            ordered.offer_page(&bullet_items),
            Err(M11BlockQuoteProjectionError::InvalidLine(
                "line record kind does not match its authenticated stream"
            ))
        ));
        accept_page(&mut ordered, &mut runtime, &ordered_items);
        let mut ordered_root = finish(&mut ordered, &mut runtime);

        assert_eq!(
            bullet_root.descriptor().projection_kind(),
            M11MarkedLineProjectionKind::BulletList
        );
        assert_eq!(
            ordered_root.descriptor().projection_kind(),
            M11MarkedLineProjectionKind::OrderedList
        );
        assert_ne!(
            bullet_root.descriptor().ordered_commitment256(),
            ordered_root.descriptor().ordered_commitment256(),
            "the authenticated stream kind must separate identical record geometry"
        );

        let version = runtime.current_source_version().expect("source");
        let (arena_root, descriptor_bytes) = ordered_root
            .transport_parts(&runtime, version, profile(13))
            .expect("ordered transport");
        assert_eq!(read_u32(&descriptor_bytes, 68).expect("wire kind"), 2);
        let descriptor = validate_persistent_block_quote_projection_root(
            runtime.producer_arena(),
            arena_root,
            &descriptor_bytes,
            version,
            profile(13),
        )
        .expect("persistent ordered descriptor");
        assert_eq!(
            descriptor.projection_kind(),
            M11MarkedLineProjectionKind::OrderedList
        );
        let mut validator = PersistentM11BlockQuoteProjectionHostValidator::new(
            runtime.producer_arena(),
            arena_root,
            descriptor,
        )
        .expect("ordered validator");
        while !validator
            .poll(runtime.producer_arena(), 1)
            .expect("ordered validation")
            .complete
        {}
        drop(validator);

        let mut cursor = ordered_root
            .cursor(
                &runtime,
                version,
                profile(13),
                0..source.len() as u32,
                0..source.len() as u32,
            )
            .expect("ordered cursor");
        let mut replay = Vec::new();
        loop {
            match cursor.poll(&runtime).expect("ordered replay") {
                M11BlockQuoteProjectionCursorPoll::Pending { .. } => {}
                M11BlockQuoteProjectionCursorPoll::Line { line, .. } => replay.push(line),
                M11BlockQuoteProjectionCursorPoll::Complete { .. } => break,
            }
        }
        assert_eq!(replay, ordered_items);
        drop(cursor);

        release(&mut bullet_root, &mut runtime);
        release(&mut ordered_root, &mut runtime);
        drop(bullet_root);
        drop(ordered_root);
        close(runtime);
    }

    #[test]
    fn requested_window_is_line_bounded_and_capped() {
        let source = format!("> {}\n", "x".repeat(BLOCK_QUOTE_WINDOW_MAX_BYTES));
        let runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        assert!(matches!(
            M11BlockQuoteProjectionBuild::new(
                &runtime,
                runtime.snapshot_current_source().expect("lease"),
                0..source.len(),
                0..source.len(),
                1,
                1,
                profile(5),
            ),
            Err(M11BlockQuoteProjectionError::WindowTooLarge { .. })
        ));
        assert!(matches!(
            M11BlockQuoteProjectionBuild::new(
                &runtime,
                runtime.snapshot_current_source().expect("lease"),
                0..source.len(),
                1..2,
                1,
                1,
                profile(5),
            ),
            Err(M11BlockQuoteProjectionError::InvalidAuthority(_))
        ));
        close(runtime);
    }
}
