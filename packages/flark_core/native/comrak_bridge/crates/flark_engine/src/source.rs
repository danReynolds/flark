use std::cell::Cell;
use std::collections::TryReserveError;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;

use crop::{Rope, RopeBuilder};

#[cfg(feature = "progressive-source-probe")]
use crate::identity::SourceLoadId;
use crate::identity::{SourceAuthority, SourceDocumentId, SourceRevision, SourceRootId};

/// Maximum source bytes copied into a cursor's reusable window.
pub const SOURCE_CURSOR_WINDOW_BYTES: usize = 4 * 1024;
/// Maximum UTF-16 units accepted in one source seed page.
pub const SOURCE_SEED_PAGE_MAX_UTF16: usize = 8 * 1024;
/// Maximum operations accepted in one atomic source edit intent.
pub const SOURCE_EDIT_MAX_OPERATIONS: usize = 1024;
/// Maximum replacement UTF-16 units accepted in one atomic source edit intent.
pub const SOURCE_EDIT_MAX_REPLACEMENT_UTF16: usize = 8 * 1024;

#[derive(Clone)]
struct SourceRoot {
    id: SourceRootId,
    rope: Rope,
}

/// The complete identity and dimensions of an immutable source snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceVersion {
    revision: SourceRevision,
    root: SourceRootId,
    byte_len: usize,
    utf16_len: usize,
}

impl SourceVersion {
    /// Reconstructs a source identity already authenticated by an external
    /// owner. The candidate host stores metrics and identity only; it never
    /// receives or aliases the parser's source rope.
    pub(crate) fn from_authenticated_parts(
        revision: SourceRevision,
        root: SourceRootId,
        byte_len: usize,
        utf16_len: usize,
    ) -> Self {
        Self {
            revision,
            root,
            byte_len,
            utf16_len,
        }
    }

    /// Returns the document-local revision.
    #[must_use]
    pub const fn revision(self) -> SourceRevision {
        self.revision
    }

    /// Returns the immutable root identity.
    #[must_use]
    pub const fn root(self) -> SourceRootId {
        self.root
    }

    /// Returns the source length in UTF-8 bytes.
    #[must_use]
    pub const fn byte_len(self) -> usize {
        self.byte_len
    }

    /// Returns the source length in UTF-16 code units.
    #[must_use]
    pub const fn utf16_len(self) -> usize {
        self.utf16_len
    }
}

/// One operation's exact coordinates in the source versions joined by an
/// [`SourceEditLineage`].
///
/// Old ranges are relative to the previous version. New ranges are relative
/// to the final current version, not to an intermediate operation result.
/// Spans are emitted in stable old-source order; same-offset insertions retain
/// their declared order.
#[derive(Debug, Eq, PartialEq)]
pub struct SourceEditLineageSpan {
    old_bytes: Range<usize>,
    new_bytes: Range<usize>,
    old_utf16: Range<usize>,
    new_utf16: Range<usize>,
}

impl SourceEditLineageSpan {
    /// Returns the replaced UTF-8 byte range in the previous version.
    #[must_use]
    pub const fn old_bytes(&self) -> &Range<usize> {
        &self.old_bytes
    }

    /// Returns the replacement UTF-8 byte range in the current version.
    #[must_use]
    pub const fn new_bytes(&self) -> &Range<usize> {
        &self.new_bytes
    }

    /// Returns the replaced UTF-16 range in the previous version.
    #[must_use]
    pub const fn old_utf16(&self) -> &Range<usize> {
        &self.old_utf16
    }

    /// Returns the replacement UTF-16 range in the current version.
    #[must_use]
    pub const fn new_utf16(&self) -> &Range<usize> {
        &self.new_utf16
    }
}

/// Chooses which side of source text inserted or replaced at an exact
/// boundary a restart/convergence cursor remains attached to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceBoundaryAffinity {
    Before,
    After,
}

/// Failure to prove that one old source range remains unchanged in the exact
/// current version joined by a [`SourceEditLineage`].
#[derive(Debug, Eq, PartialEq)]
pub enum SourceEditLineageError {
    /// The supplied previous version is not the lineage's exact old authority.
    PreviousVersionMismatch {
        expected: SourceVersion,
        actual: SourceVersion,
    },
    /// The supplied current version is not the lineage's exact new authority.
    CurrentVersionMismatch {
        expected: SourceVersion,
        actual: SourceVersion,
    },
    /// The old byte range was reversed or outside the previous source.
    InvalidByteRange {
        start: usize,
        end: usize,
        len: usize,
    },
    /// Empty ranges need an explicit boundary-affinity API and are not range
    /// reuse evidence.
    EmptyByteRange { offset: usize },
    /// A byte boundary was outside the previous source.
    InvalidByteBoundary { offset: usize, len: usize },
    /// The old UTF-16 range was reversed or outside the previous source.
    InvalidUtf16Range {
        start: usize,
        end: usize,
        len: usize,
    },
    /// Empty ranges need an explicit boundary-affinity API and are not range
    /// reuse evidence.
    EmptyUtf16Range { offset: usize },
    /// A UTF-16 boundary was outside the previous source.
    InvalidUtf16Boundary { offset: usize, len: usize },
    /// The byte range intersects an edit or crosses an insertion.
    EditedByteRange {
        start: usize,
        end: usize,
        span_index: usize,
    },
    /// The UTF-16 range intersects an edit or crosses an insertion.
    EditedUtf16Range {
        start: usize,
        end: usize,
        span_index: usize,
    },
    /// A byte boundary fell strictly inside replaced source.
    EditedByteBoundary { offset: usize, span_index: usize },
    /// A UTF-16 boundary fell strictly inside replaced source.
    EditedUtf16Boundary { offset: usize, span_index: usize },
    /// A coordinate mapping exceeded the host representation.
    MetricOverflow,
}

impl fmt::Display for SourceEditLineageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PreviousVersionMismatch { .. } => {
                formatter.write_str("source lineage previous version mismatch")
            }
            Self::CurrentVersionMismatch { .. } => {
                formatter.write_str("source lineage current version mismatch")
            }
            Self::InvalidByteRange { start, end, len } => write!(
                formatter,
                "invalid lineage byte range {start}..{end} for source length {len}"
            ),
            Self::EmptyByteRange { offset } => {
                write!(formatter, "lineage byte range at {offset} is empty")
            }
            Self::InvalidByteBoundary { offset, len } => write!(
                formatter,
                "invalid lineage byte boundary {offset} for source length {len}"
            ),
            Self::InvalidUtf16Range { start, end, len } => write!(
                formatter,
                "invalid lineage UTF-16 range {start}..{end} for source length {len}"
            ),
            Self::EmptyUtf16Range { offset } => {
                write!(formatter, "lineage UTF-16 range at {offset} is empty")
            }
            Self::InvalidUtf16Boundary { offset, len } => write!(
                formatter,
                "invalid lineage UTF-16 boundary {offset} for source length {len}"
            ),
            Self::EditedByteRange {
                start,
                end,
                span_index,
            } => write!(
                formatter,
                "lineage byte range {start}..{end} crosses edit span {span_index}"
            ),
            Self::EditedUtf16Range {
                start,
                end,
                span_index,
            } => write!(
                formatter,
                "lineage UTF-16 range {start}..{end} crosses edit span {span_index}"
            ),
            Self::EditedByteBoundary { offset, span_index } => write!(
                formatter,
                "lineage byte boundary {offset} falls inside edit span {span_index}"
            ),
            Self::EditedUtf16Boundary { offset, span_index } => write!(
                formatter,
                "lineage UTF-16 boundary {offset} falls inside edit span {span_index}"
            ),
            Self::MetricOverflow => formatter.write_str("source lineage metric overflowed"),
        }
    }
}

impl std::error::Error for SourceEditLineageError {}

/// Move-only scalar provenance for one committed source transition.
///
/// This capability is minted only by [`SourceStore`] commit paths. It owns no
/// source text, Crop root, source lease, or weak source handle. Supplying both
/// versions to a mapping call prevents a range from being accidentally mapped
/// through crossed document or revision authority.
pub struct SourceEditLineage {
    previous: SourceVersion,
    current: SourceVersion,
    spans: Vec<SourceEditLineageSpan>,
}

impl fmt::Debug for SourceEditLineage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceEditLineage")
            .field("previous", &self.previous)
            .field("current", &self.current)
            .field("spans", &self.spans)
            .finish()
    }
}

impl SourceEditLineage {
    fn new(
        previous: SourceVersion,
        current: SourceVersion,
        spans: Vec<SourceEditLineageSpan>,
    ) -> Self {
        debug_assert!(!spans.is_empty());
        Self {
            previous,
            current,
            spans,
        }
    }

    /// Returns the exact source version replaced by this commit.
    #[must_use]
    pub const fn previous(&self) -> SourceVersion {
        self.previous
    }

    /// Returns the exact source version installed by this commit.
    #[must_use]
    pub const fn current(&self) -> SourceVersion {
        self.current
    }

    /// Returns operation spans in stable previous-source order.
    #[must_use]
    pub fn spans(&self) -> &[SourceEditLineageSpan] {
        &self.spans
    }

    /// Maps a non-empty unchanged byte range from `previous` into `current`.
    ///
    /// A range that intersects a replacement/deletion or strictly crosses an
    /// insertion is rejected. A range ending at an insertion remains on its
    /// old side; a range starting there maps after the insertion.
    pub fn map_unchanged_byte_range(
        &self,
        previous: SourceVersion,
        current: SourceVersion,
        range: Range<usize>,
    ) -> Result<Range<usize>, SourceEditLineageError> {
        self.validate_versions(previous, current)?;
        self.map_unchanged_range(range, LineageCoordinate::Byte)
    }

    /// Maps a non-empty unchanged UTF-16 range from `previous` into `current`.
    ///
    /// UTF-16 spans are derived by the source store at commit preparation, so
    /// callers never need source text to translate the range.
    pub fn map_unchanged_utf16_range(
        &self,
        previous: SourceVersion,
        current: SourceVersion,
        range: Range<usize>,
    ) -> Result<Range<usize>, SourceEditLineageError> {
        self.validate_versions(previous, current)?;
        self.map_unchanged_range(range, LineageCoordinate::Utf16)
    }

    /// Maps one exact byte boundary through this transition.
    ///
    /// A boundary touching replaced or inserted text maps to the selected
    /// side of the entire touching edit cluster. A boundary strictly inside a
    /// replaced range is not unchanged and is rejected.
    pub fn map_byte_boundary(
        &self,
        previous: SourceVersion,
        current: SourceVersion,
        offset: usize,
        affinity: SourceBoundaryAffinity,
    ) -> Result<usize, SourceEditLineageError> {
        self.validate_versions(previous, current)?;
        self.map_boundary(offset, affinity, LineageCoordinate::Byte)
    }

    /// Maps one exact UTF-16 boundary through this transition.
    pub fn map_utf16_boundary(
        &self,
        previous: SourceVersion,
        current: SourceVersion,
        offset: usize,
        affinity: SourceBoundaryAffinity,
    ) -> Result<usize, SourceEditLineageError> {
        self.validate_versions(previous, current)?;
        self.map_boundary(offset, affinity, LineageCoordinate::Utf16)
    }

    fn validate_versions(
        &self,
        previous: SourceVersion,
        current: SourceVersion,
    ) -> Result<(), SourceEditLineageError> {
        if previous != self.previous {
            return Err(SourceEditLineageError::PreviousVersionMismatch {
                expected: self.previous,
                actual: previous,
            });
        }
        if current != self.current {
            return Err(SourceEditLineageError::CurrentVersionMismatch {
                expected: self.current,
                actual: current,
            });
        }
        Ok(())
    }

    fn map_unchanged_range(
        &self,
        range: Range<usize>,
        coordinate: LineageCoordinate,
    ) -> Result<Range<usize>, SourceEditLineageError> {
        let (previous_len, current_len) = match coordinate {
            LineageCoordinate::Byte => (self.previous.byte_len, self.current.byte_len),
            LineageCoordinate::Utf16 => (self.previous.utf16_len, self.current.utf16_len),
        };
        if range.start > range.end || range.end > previous_len {
            return Err(match coordinate {
                LineageCoordinate::Byte => SourceEditLineageError::InvalidByteRange {
                    start: range.start,
                    end: range.end,
                    len: previous_len,
                },
                LineageCoordinate::Utf16 => SourceEditLineageError::InvalidUtf16Range {
                    start: range.start,
                    end: range.end,
                    len: previous_len,
                },
            });
        }
        if range.is_empty() {
            return Err(match coordinate {
                LineageCoordinate::Byte => SourceEditLineageError::EmptyByteRange {
                    offset: range.start,
                },
                LineageCoordinate::Utf16 => SourceEditLineageError::EmptyUtf16Range {
                    offset: range.start,
                },
            });
        }

        let mut old_anchor = 0_usize;
        let mut new_anchor = 0_usize;
        for (span_index, span) in self.spans.iter().enumerate() {
            let (old, new) = match coordinate {
                LineageCoordinate::Byte => (&span.old_bytes, &span.new_bytes),
                LineageCoordinate::Utf16 => (&span.old_utf16, &span.new_utf16),
            };

            let edit_is_before_range = if old.is_empty() {
                if range.end <= old.start {
                    false
                } else if range.start >= old.start {
                    true
                } else {
                    return Err(edited_range_error(coordinate, &range, span_index));
                }
            } else if range.end <= old.start {
                false
            } else if range.start >= old.end {
                true
            } else {
                return Err(edited_range_error(coordinate, &range, span_index));
            };

            if edit_is_before_range {
                old_anchor = old.end;
                new_anchor = new.end;
            } else {
                break;
            }
        }

        let start_delta = range
            .start
            .checked_sub(old_anchor)
            .ok_or(SourceEditLineageError::MetricOverflow)?;
        let end_delta = range
            .end
            .checked_sub(old_anchor)
            .ok_or(SourceEditLineageError::MetricOverflow)?;
        let mapped_start = new_anchor
            .checked_add(start_delta)
            .ok_or(SourceEditLineageError::MetricOverflow)?;
        let mapped_end = new_anchor
            .checked_add(end_delta)
            .ok_or(SourceEditLineageError::MetricOverflow)?;
        if mapped_start > mapped_end || mapped_end > current_len {
            return Err(SourceEditLineageError::MetricOverflow);
        }
        Ok(mapped_start..mapped_end)
    }

    fn map_boundary(
        &self,
        offset: usize,
        affinity: SourceBoundaryAffinity,
        coordinate: LineageCoordinate,
    ) -> Result<usize, SourceEditLineageError> {
        let (previous_len, current_len) = match coordinate {
            LineageCoordinate::Byte => (self.previous.byte_len, self.current.byte_len),
            LineageCoordinate::Utf16 => (self.previous.utf16_len, self.current.utf16_len),
        };
        if offset > previous_len {
            return Err(match coordinate {
                LineageCoordinate::Byte => SourceEditLineageError::InvalidByteBoundary {
                    offset,
                    len: previous_len,
                },
                LineageCoordinate::Utf16 => SourceEditLineageError::InvalidUtf16Boundary {
                    offset,
                    len: previous_len,
                },
            });
        }

        let mut old_anchor = 0_usize;
        let mut new_anchor = 0_usize;
        let mut index = 0_usize;
        while let Some(span) = self.spans.get(index) {
            let (old, new) = match coordinate {
                LineageCoordinate::Byte => (&span.old_bytes, &span.new_bytes),
                LineageCoordinate::Utf16 => (&span.old_utf16, &span.new_utf16),
            };
            if offset < old.start {
                break;
            }
            if offset > old.end {
                old_anchor = old.end;
                new_anchor = new.end;
                index += 1;
                continue;
            }
            if offset > old.start && offset < old.end {
                return Err(edited_boundary_error(coordinate, offset, index));
            }

            let first_new_start = new.start;
            let mut last_new_end = new.end;
            let mut touching_index = index + 1;
            while let Some(touching) = self.spans.get(touching_index) {
                let (touching_old, touching_new) = match coordinate {
                    LineageCoordinate::Byte => (&touching.old_bytes, &touching.new_bytes),
                    LineageCoordinate::Utf16 => (&touching.old_utf16, &touching.new_utf16),
                };
                if offset < touching_old.start || offset > touching_old.end {
                    break;
                }
                if offset > touching_old.start && offset < touching_old.end {
                    return Err(edited_boundary_error(coordinate, offset, touching_index));
                }
                last_new_end = touching_new.end;
                touching_index += 1;
            }
            let mapped = match affinity {
                SourceBoundaryAffinity::Before => first_new_start,
                SourceBoundaryAffinity::After => last_new_end,
            };
            return (mapped <= current_len)
                .then_some(mapped)
                .ok_or(SourceEditLineageError::MetricOverflow);
        }

        let mapped = new_anchor
            .checked_add(
                offset
                    .checked_sub(old_anchor)
                    .ok_or(SourceEditLineageError::MetricOverflow)?,
            )
            .ok_or(SourceEditLineageError::MetricOverflow)?;
        (mapped <= current_len)
            .then_some(mapped)
            .ok_or(SourceEditLineageError::MetricOverflow)
    }
}

#[derive(Clone, Copy)]
enum LineageCoordinate {
    Byte,
    Utf16,
}

fn edited_range_error(
    coordinate: LineageCoordinate,
    range: &Range<usize>,
    span_index: usize,
) -> SourceEditLineageError {
    match coordinate {
        LineageCoordinate::Byte => SourceEditLineageError::EditedByteRange {
            start: range.start,
            end: range.end,
            span_index,
        },
        LineageCoordinate::Utf16 => SourceEditLineageError::EditedUtf16Range {
            start: range.start,
            end: range.end,
            span_index,
        },
    }
}

fn edited_boundary_error(
    coordinate: LineageCoordinate,
    offset: usize,
    span_index: usize,
) -> SourceEditLineageError {
    match coordinate {
        LineageCoordinate::Byte => {
            SourceEditLineageError::EditedByteBoundary { offset, span_index }
        }
        LineageCoordinate::Utf16 => {
            SourceEditLineageError::EditedUtf16Boundary { offset, span_index }
        }
    }
}

/// Failure while validating, preparing, or committing a source edit.
#[derive(Debug, Eq, PartialEq)]
pub enum SourceEditError {
    /// The caller edited a source version that is no longer current.
    StaleVersion {
        expected: SourceVersion,
        actual: SourceVersion,
    },
    /// The byte range was reversed or outside the current source.
    InvalidRange {
        start: usize,
        end: usize,
        len: usize,
    },
    /// A line read escaped the exact range selected for its discovery cursor.
    OutsideCursorRange {
        start: usize,
        end: usize,
        allowed_start: usize,
        allowed_end: usize,
    },
    /// A bounded source cursor was finished before its selected range ended.
    IncompleteCursor { expected: usize, actual: usize },
    /// One of the edit boundaries split a UTF-8 scalar.
    SplitUtf8Scalar { offset: usize },
    /// A UTF-16 offset was outside the immutable source.
    InvalidUtf16Offset { offset: usize, len: usize },
    /// A UTF-16 offset split the surrogate pair for one Unicode scalar.
    SplitUtf16Scalar { offset: usize },
    /// A UTF-16 range was reversed or outside the immutable source.
    InvalidUtf16Range {
        start: usize,
        end: usize,
        len: usize,
    },
    /// Operations were not in stable source order or overlapped.
    InvalidOperationOrder {
        previous_start: usize,
        previous_end: usize,
        start: usize,
        end: usize,
    },
    /// A source intent contained no operations.
    EmptyEditIntent,
    /// The externally declared target was not exactly the next revision.
    InvalidRevisionTransition {
        current: SourceRevision,
        declared: SourceRevision,
    },
    /// A source seed page did not exactly continue the declared UTF-16 range.
    InvalidSeedPage {
        expected_start: usize,
        start: usize,
        end: usize,
        page_utf16_len: usize,
        expected_total: usize,
    },
    /// A source seed page exceeded the bounded page contract.
    SeedPageTooLarge { observed: usize, limit: usize },
    /// A source seed ended before its declared UTF-16 length.
    IncompleteSeed { expected: usize, observed: usize },
    /// An earlier malformed page permanently invalidated this seed attempt.
    SeedPoisoned,
    /// A source byte or UTF-16 metric overflowed the host representation.
    MetricOverflow,
    /// An edit intent exceeded the bounded operation count.
    TooManyEditOperations { observed: usize, limit: usize },
    /// An edit intent exceeded the bounded replacement payload.
    EditReplacementTooLarge { observed: usize, limit: usize },
    /// A monotonic identity or revision counter was exhausted.
    IdentityExhausted,
    /// A bounded allocation could not be satisfied.
    AllocationFailed,
}

impl fmt::Display for SourceEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleVersion { .. } => formatter.write_str("source version is stale"),
            Self::InvalidRange { start, end, len } => {
                write!(
                    formatter,
                    "invalid byte range {start}..{end} for source length {len}"
                )
            }
            Self::OutsideCursorRange {
                start,
                end,
                allowed_start,
                allowed_end,
            } => write!(
                formatter,
                "line access {start}..{end} escapes cursor range {allowed_start}..{allowed_end}"
            ),
            Self::IncompleteCursor { expected, actual } => write!(
                formatter,
                "source cursor stopped at byte {actual} before its range end {expected}"
            ),
            Self::SplitUtf8Scalar { offset } => {
                write!(formatter, "byte offset {offset} splits a UTF-8 scalar")
            }
            Self::InvalidUtf16Offset { offset, len } => {
                write!(
                    formatter,
                    "invalid UTF-16 offset {offset} for source length {len}"
                )
            }
            Self::SplitUtf16Scalar { offset } => {
                write!(formatter, "UTF-16 offset {offset} splits a surrogate pair")
            }
            Self::InvalidUtf16Range { start, end, len } => {
                write!(
                    formatter,
                    "invalid UTF-16 range {start}..{end} for source length {len}"
                )
            }
            Self::InvalidOperationOrder {
                previous_start,
                previous_end,
                start,
                end,
            } => write!(
                formatter,
                "UTF-16 edit {start}..{end} is not ordered after {previous_start}..{previous_end}"
            ),
            Self::EmptyEditIntent => formatter.write_str("source edit intent is empty"),
            Self::InvalidRevisionTransition { .. } => {
                formatter.write_str("source edit did not declare exactly the next revision")
            }
            Self::InvalidSeedPage { .. } => {
                formatter.write_str("source seed page is out of order or has invalid metrics")
            }
            Self::SeedPageTooLarge { observed, limit } => write!(
                formatter,
                "source seed page has {observed} UTF-16 units but the limit is {limit}"
            ),
            Self::IncompleteSeed { expected, observed } => write!(
                formatter,
                "source seed observed {observed} UTF-16 units but expected {expected}"
            ),
            Self::SeedPoisoned => formatter.write_str("source seed is poisoned"),
            Self::MetricOverflow => formatter.write_str("source metric overflowed"),
            Self::TooManyEditOperations { observed, limit } => write!(
                formatter,
                "source edit has {observed} operations but the limit is {limit}"
            ),
            Self::EditReplacementTooLarge { observed, limit } => write!(
                formatter,
                "source edit has {observed} replacement UTF-16 units but the limit is {limit}"
            ),
            Self::IdentityExhausted => formatter.write_str("source identity space is exhausted"),
            Self::AllocationFailed => formatter.write_str("source cursor allocation failed"),
        }
    }
}

impl std::error::Error for SourceEditError {}

impl From<TryReserveError> for SourceEditError {
    fn from(_: TryReserveError) -> Self {
        Self::AllocationFailed
    }
}

/// An unpublished, paged construction of one exact source replica.
///
/// Pages are appended in UTF-16 coordinate order. No [`SourceStore`] exists
/// until [`finalize`](Self::finalize) validates the declared total and consumes
/// this builder. A malformed page poisons the attempt so a transport cannot
/// silently recover from a partial or reordered seed.
pub struct SourceSeedBuilder {
    revision: SourceRevision,
    expected_utf16_len: usize,
    observed_byte_len: usize,
    observed_utf16_len: usize,
    builder: RopeBuilder,
    poisoned: bool,
    _not_sync: PhantomData<Cell<()>>,
}

impl fmt::Debug for SourceSeedBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceSeedBuilder")
            .field("revision", &self.revision)
            .field("expected_utf16_len", &self.expected_utf16_len)
            .field("observed_byte_len", &self.observed_byte_len)
            .field("observed_utf16_len", &self.observed_utf16_len)
            .field("poisoned", &self.poisoned)
            .finish_non_exhaustive()
    }
}

impl SourceSeedBuilder {
    /// Starts an unpublished seed for an externally assigned revision.
    #[must_use]
    pub fn new(revision: SourceRevision, expected_utf16_len: usize) -> Self {
        Self {
            revision,
            expected_utf16_len,
            observed_byte_len: 0,
            observed_utf16_len: 0,
            builder: RopeBuilder::new(),
            poisoned: false,
            _not_sync: PhantomData,
        }
    }

    /// Returns the externally assigned revision for this seed.
    #[must_use]
    pub const fn revision(&self) -> SourceRevision {
        self.revision
    }

    /// Returns the declared final UTF-16 length.
    #[must_use]
    pub const fn expected_utf16_len(&self) -> usize {
        self.expected_utf16_len
    }

    /// Returns the exact UTF-8 bytes observed in accepted pages.
    #[must_use]
    pub const fn observed_byte_len(&self) -> usize {
        self.observed_byte_len
    }

    /// Returns the exact UTF-16 units observed in accepted pages.
    #[must_use]
    pub const fn observed_utf16_len(&self) -> usize {
        self.observed_utf16_len
    }

    /// Appends one page whose coordinates are relative to the final source.
    pub fn append_page(
        &mut self,
        utf16_range: Range<usize>,
        text: &str,
    ) -> Result<(), SourceEditError> {
        if self.poisoned {
            return Err(SourceEditError::SeedPoisoned);
        }

        let page_utf16_len = text.encode_utf16().count();
        if page_utf16_len > SOURCE_SEED_PAGE_MAX_UTF16 {
            self.poisoned = true;
            return Err(SourceEditError::SeedPageTooLarge {
                observed: page_utf16_len,
                limit: SOURCE_SEED_PAGE_MAX_UTF16,
            });
        }
        let expected_end = utf16_range.start.checked_add(page_utf16_len);
        let valid_empty_page = self.expected_utf16_len == 0
            && utf16_range.start == 0
            && utf16_range.end == 0
            && text.is_empty();
        let valid_progress = !text.is_empty() && page_utf16_len > 0;
        if utf16_range.start != self.observed_utf16_len
            || expected_end != Some(utf16_range.end)
            || utf16_range.end > self.expected_utf16_len
            || (!valid_empty_page && !valid_progress)
        {
            self.poisoned = true;
            return Err(SourceEditError::InvalidSeedPage {
                expected_start: self.observed_utf16_len,
                start: utf16_range.start,
                end: utf16_range.end,
                page_utf16_len,
                expected_total: self.expected_utf16_len,
            });
        }

        let observed_byte_len =
            self.observed_byte_len
                .checked_add(text.len())
                .ok_or_else(|| {
                    self.poisoned = true;
                    SourceEditError::MetricOverflow
                })?;
        let observed_utf16_len = self
            .observed_utf16_len
            .checked_add(page_utf16_len)
            .ok_or_else(|| {
                self.poisoned = true;
                SourceEditError::MetricOverflow
            })?;

        self.builder.append(text);
        self.observed_byte_len = observed_byte_len;
        self.observed_utf16_len = observed_utf16_len;
        Ok(())
    }

    /// Validates the complete seed and publishes its first [`SourceStore`].
    pub fn finalize(self) -> Result<SourceStore, SourceEditError> {
        if self.poisoned {
            return Err(SourceEditError::SeedPoisoned);
        }
        if self.observed_utf16_len != self.expected_utf16_len {
            return Err(SourceEditError::IncompleteSeed {
                expected: self.expected_utf16_len,
                observed: self.observed_utf16_len,
            });
        }

        let rope = self.builder.build();
        if rope.byte_len() != self.observed_byte_len || rope.utf16_len() != self.observed_utf16_len
        {
            return Err(SourceEditError::MetricOverflow);
        }
        let id = SourceRootId::allocate().ok_or(SourceEditError::IdentityExhausted)?;
        let document =
            SourceDocumentId::allocate().ok_or(SourceEditError::IdentityExhausted)?;
        Ok(SourceStore {
            document,
            revision: self.revision,
            root: Arc::new(SourceRoot { id, rope }),
            _not_sync: PhantomData,
        })
    }
}

/// Exact authority of one published prefix during progressive source loading.
///
/// `generation` advances for both stream appends and admitted-prefix edits.
/// `revision` advances only for user edits. This keeps transport progress and
/// edit history as separate axes while every published immutable root remains
/// unambiguous.
#[cfg(feature = "progressive-source-probe")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpeningSourceVersion {
    load: SourceLoadId,
    generation: u64,
    revision: SourceRevision,
    root: SourceRootId,
    admitted_input_bytes: usize,
    admitted_input_utf16: usize,
    expected_input_utf16: usize,
    current_bytes: usize,
    current_utf16: usize,
}

#[cfg(feature = "progressive-source-probe")]
impl OpeningSourceVersion {
    #[must_use]
    pub const fn load(self) -> SourceLoadId {
        self.load
    }

    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn revision(self) -> SourceRevision {
        self.revision
    }

    #[must_use]
    pub const fn root(self) -> SourceRootId {
        self.root
    }

    #[must_use]
    pub const fn admitted_input_bytes(self) -> usize {
        self.admitted_input_bytes
    }

    #[must_use]
    pub const fn admitted_input_utf16(self) -> usize {
        self.admitted_input_utf16
    }

    #[must_use]
    pub const fn expected_input_utf16(self) -> usize {
        self.expected_input_utf16
    }

    #[must_use]
    pub const fn current_bytes(self) -> usize {
        self.current_bytes
    }

    #[must_use]
    pub const fn current_utf16(self) -> usize {
        self.current_utf16
    }

    #[must_use]
    pub const fn input_complete(self) -> bool {
        self.admitted_input_utf16 == self.expected_input_utf16
    }
}

/// One immutable, readable prefix snapshot paired with its opening authority.
#[cfg(feature = "progressive-source-probe")]
pub struct OpeningSourceSnapshot {
    opening: OpeningSourceVersion,
    source: SourceSnapshotLease,
}

/// Store-minted proof that one newer opening snapshot only appends to the
/// exact previous snapshot under the same logical edit authority.
///
/// The proof is move-only and owns the newer readable snapshot. Consumers do
/// not infer append safety from matching coordinates, generations, or roots.
#[cfg(feature = "progressive-source-probe")]
pub struct OpeningSourceAppendProof {
    previous: OpeningSourceVersion,
    current: OpeningSourceVersion,
    snapshot: OpeningSourceSnapshot,
}

#[cfg(feature = "progressive-source-probe")]
impl fmt::Debug for OpeningSourceAppendProof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpeningSourceAppendProof")
            .field("previous", &self.previous)
            .field("current", &self.current)
            .field("authority", &self.authority())
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "progressive-source-probe")]
impl OpeningSourceAppendProof {
    #[must_use]
    pub const fn previous(&self) -> OpeningSourceVersion {
        self.previous
    }

    #[must_use]
    pub const fn current(&self) -> OpeningSourceVersion {
        self.current
    }

    #[must_use]
    pub const fn authority(&self) -> SourceAuthority {
        self.snapshot.authority()
    }

    #[must_use]
    pub fn previous_source_version(&self) -> SourceVersion {
        source_version_for_opening(self.previous)
    }

    #[must_use]
    pub fn current_source_version(&self) -> SourceVersion {
        source_version_for_opening(self.current)
    }

    /// Returns whether the admitted frontier can be exposed to a parser as an
    /// unsealed physical-line boundary. A sealed final frontier may instead
    /// terminate the last line without a line ending.
    pub fn current_ends_at_physical_line_boundary(&self) -> Result<bool, SourceEditError> {
        self.snapshot
            .source
            .is_physical_line_start(self.current.current_bytes)
    }

    fn into_parts(
        self,
    ) -> (
        OpeningSourceVersion,
        OpeningSourceVersion,
        OpeningSourceSnapshot,
    ) {
        (self.previous, self.current, self.snapshot)
    }
}

#[cfg(feature = "progressive-source-probe")]
impl OpeningSourceSnapshot {
    #[must_use]
    pub const fn opening_version(&self) -> OpeningSourceVersion {
        self.opening
    }

    #[must_use]
    pub fn source_version(&self) -> SourceVersion {
        self.source.version()
    }

    /// Returns the logical document/revision authority shared by compatible
    /// append generations.
    #[must_use]
    pub const fn authority(&self) -> SourceAuthority {
        self.source.authority()
    }

    #[must_use]
    pub fn into_source_lease(self) -> SourceSnapshotLease {
        self.source
    }

    pub(crate) fn into_source_store_replica(self) -> SourceStore {
        SourceStore {
            document: self.source.document,
            revision: self.source.revision,
            root: self.source.root,
            _not_sync: PhantomData,
        }
    }
}

/// Failure while operating one progressive source-admission transaction.
#[cfg(feature = "progressive-source-probe")]
#[derive(Debug, Eq, PartialEq)]
pub enum OpeningSourceError {
    StaleVersion {
        expected: OpeningSourceVersion,
        actual: OpeningSourceVersion,
    },
    NotAppendLineage {
        previous: OpeningSourceVersion,
        current: OpeningSourceVersion,
    },
    ForeignAuthority,
    Source(SourceEditError),
}

#[cfg(feature = "progressive-source-probe")]
impl fmt::Display for OpeningSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleVersion { .. } => formatter.write_str("opening source version is stale"),
            Self::NotAppendLineage { .. } => {
                formatter.write_str("opening source generations do not prove append-only lineage")
            }
            Self::ForeignAuthority => {
                formatter.write_str("opening source proof belongs to a different document")
            }
            Self::Source(error) => write!(formatter, "opening source failed: {error}"),
        }
    }
}

#[cfg(feature = "progressive-source-probe")]
impl std::error::Error for OpeningSourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::StaleVersion { .. }
            | Self::NotAppendLineage { .. }
            | Self::ForeignAuthority => None,
            Self::Source(error) => Some(error),
        }
    }
}

#[cfg(feature = "progressive-source-probe")]
impl From<SourceEditError> for OpeningSourceError {
    fn from(error: SourceEditError) -> Self {
        Self::Source(error)
    }
}

/// Append-published exact source used by RFC 029 Experiment A1.
///
/// Each mutation creates one immutable Crop root by structural sharing. Old
/// snapshots remain readable; sealing consumes the current root directly into
/// [`SourceStore`] without rebuilding or copying the complete source.
#[cfg(feature = "progressive-source-probe")]
pub struct OpeningSourceStore {
    load: SourceLoadId,
    document: SourceDocumentId,
    generation: u64,
    revision: SourceRevision,
    expected_input_utf16: usize,
    admitted_input_bytes: usize,
    admitted_input_utf16: usize,
    root: Arc<SourceRoot>,
    poisoned: bool,
    _not_sync: PhantomData<Cell<()>>,
}

#[cfg(feature = "progressive-source-probe")]
impl fmt::Debug for OpeningSourceStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpeningSourceStore")
            .field("version", &self.version())
            .field("poisoned", &self.poisoned)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "progressive-source-probe")]
impl OpeningSourceStore {
    pub fn new(
        revision: SourceRevision,
        expected_input_utf16: usize,
    ) -> Result<Self, OpeningSourceError> {
        let load = SourceLoadId::allocate().ok_or(SourceEditError::IdentityExhausted)?;
        let document =
            SourceDocumentId::allocate().ok_or(SourceEditError::IdentityExhausted)?;
        let root = SourceRootId::allocate().ok_or(SourceEditError::IdentityExhausted)?;
        Ok(Self {
            load,
            document,
            generation: 0,
            revision,
            expected_input_utf16,
            admitted_input_bytes: 0,
            admitted_input_utf16: 0,
            root: Arc::new(SourceRoot {
                id: root,
                rope: Rope::from(""),
            }),
            poisoned: false,
            _not_sync: PhantomData,
        })
    }

    #[must_use]
    pub fn version(&self) -> OpeningSourceVersion {
        OpeningSourceVersion {
            load: self.load,
            generation: self.generation,
            revision: self.revision,
            root: self.root.id,
            admitted_input_bytes: self.admitted_input_bytes,
            admitted_input_utf16: self.admitted_input_utf16,
            expected_input_utf16: self.expected_input_utf16,
            current_bytes: self.root.rope.byte_len(),
            current_utf16: self.root.rope.utf16_len(),
        }
    }

    /// Returns semantic continuity independent of the current append root.
    #[must_use]
    pub const fn authority(&self) -> SourceAuthority {
        SourceAuthority::new(self.document, self.revision)
    }

    #[must_use]
    pub fn snapshot(&self) -> OpeningSourceSnapshot {
        OpeningSourceSnapshot {
            opening: self.version(),
            source: SourceSnapshotLease {
                document: self.document,
                revision: self.revision,
                root: Arc::clone(&self.root),
                _not_sync: PhantomData,
            },
        }
    }

    /// Mints an append-only lineage proof from an earlier published version to
    /// the exact current snapshot. Any intervening user edit changes the edit
    /// revision and fails closed, even if later text happens to match.
    pub fn prove_append_since(
        &self,
        previous: OpeningSourceVersion,
    ) -> Result<OpeningSourceAppendProof, OpeningSourceError> {
        let current = self.version();
        let is_append = previous.load == current.load
            && previous.generation < current.generation
            && previous.revision == current.revision
            && previous.admitted_input_bytes < current.admitted_input_bytes
            && previous.admitted_input_utf16 < current.admitted_input_utf16
            && previous.current_bytes < current.current_bytes
            && previous.current_utf16 < current.current_utf16;
        if !is_append {
            return Err(OpeningSourceError::NotAppendLineage { previous, current });
        }
        Ok(OpeningSourceAppendProof {
            previous,
            current,
            snapshot: self.snapshot(),
        })
    }

    /// Publishes one bounded continuation of the original input stream.
    pub fn append_page(
        &mut self,
        expected: OpeningSourceVersion,
        input_utf16_range: Range<usize>,
        text: &str,
    ) -> Result<OpeningSourceVersion, OpeningSourceError> {
        self.validate_expected(expected)?;
        if self.poisoned {
            return Err(SourceEditError::SeedPoisoned.into());
        }

        let page_utf16_len = text.encode_utf16().count();
        if page_utf16_len > SOURCE_SEED_PAGE_MAX_UTF16 {
            self.poisoned = true;
            return Err(SourceEditError::SeedPageTooLarge {
                observed: page_utf16_len,
                limit: SOURCE_SEED_PAGE_MAX_UTF16,
            }
            .into());
        }
        let expected_end = input_utf16_range.start.checked_add(page_utf16_len);
        if text.is_empty()
            || page_utf16_len == 0
            || input_utf16_range.start != self.admitted_input_utf16
            || expected_end != Some(input_utf16_range.end)
            || input_utf16_range.end > self.expected_input_utf16
        {
            self.poisoned = true;
            return Err(SourceEditError::InvalidSeedPage {
                expected_start: self.admitted_input_utf16,
                start: input_utf16_range.start,
                end: input_utf16_range.end,
                page_utf16_len,
                expected_total: self.expected_input_utf16,
            }
            .into());
        }

        let admitted_input_bytes = self
            .admitted_input_bytes
            .checked_add(text.len())
            .ok_or(SourceEditError::MetricOverflow)?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(SourceEditError::IdentityExhausted)?;
        let id = SourceRootId::allocate().ok_or(SourceEditError::IdentityExhausted)?;
        let mut rope = self.root.rope.clone();
        let append_at = rope.byte_len();
        rope.replace(append_at..append_at, text);

        self.root = Arc::new(SourceRoot { id, rope });
        self.generation = generation;
        self.admitted_input_bytes = admitted_input_bytes;
        self.admitted_input_utf16 = input_utf16_range.end;
        Ok(self.version())
    }

    /// Applies one bounded user edit entirely inside currently admitted text.
    pub fn apply_utf16_edit(
        &mut self,
        expected: OpeningSourceVersion,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<OpeningSourceVersion, OpeningSourceError> {
        self.validate_expected(expected)?;
        if self.poisoned {
            return Err(SourceEditError::SeedPoisoned.into());
        }
        let current_utf16 = self.root.rope.utf16_len();
        if range.start > range.end || range.end > current_utf16 {
            return Err(SourceEditError::InvalidUtf16Range {
                start: range.start,
                end: range.end,
                len: current_utf16,
            }
            .into());
        }
        let replacement_utf16 = replacement.encode_utf16().count();
        if replacement_utf16 > SOURCE_EDIT_MAX_REPLACEMENT_UTF16 {
            return Err(SourceEditError::EditReplacementTooLarge {
                observed: replacement_utf16,
                limit: SOURCE_EDIT_MAX_REPLACEMENT_UTF16,
            }
            .into());
        }

        let start_byte = checked_byte_offset_for_utf16(&self.root.rope, range.start)?;
        let end_byte = checked_byte_offset_for_utf16(&self.root.rope, range.end)?;
        let revision = self
            .revision
            .checked_next()
            .ok_or(SourceEditError::IdentityExhausted)?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(SourceEditError::IdentityExhausted)?;
        let id = SourceRootId::allocate().ok_or(SourceEditError::IdentityExhausted)?;
        let mut rope = self.root.rope.clone();
        rope.replace(start_byte..end_byte, replacement);

        self.root = Arc::new(SourceRoot { id, rope });
        self.revision = revision;
        self.generation = generation;
        Ok(self.version())
    }

    /// Seals EOF and promotes the current immutable root without rebuilding it.
    pub fn seal(self) -> Result<SourceStore, OpeningSourceError> {
        if self.poisoned {
            return Err(SourceEditError::SeedPoisoned.into());
        }
        if self.admitted_input_utf16 != self.expected_input_utf16 {
            return Err(SourceEditError::IncompleteSeed {
                expected: self.expected_input_utf16,
                observed: self.admitted_input_utf16,
            }
            .into());
        }
        Ok(SourceStore {
            document: self.document,
            revision: self.revision,
            root: self.root,
            _not_sync: PhantomData,
        })
    }

    fn validate_expected(&self, expected: OpeningSourceVersion) -> Result<(), OpeningSourceError> {
        let actual = self.version();
        if expected != actual {
            return Err(OpeningSourceError::StaleVersion { expected, actual });
        }
        Ok(())
    }
}

/// One borrowed source operation expressed in base-revision UTF-16 offsets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceUtf16Operation<'a> {
    range: Range<usize>,
    replacement: &'a str,
}

impl<'a> SourceUtf16Operation<'a> {
    /// Creates one operation against the unchanged base revision.
    #[must_use]
    pub const fn new(range: Range<usize>, replacement: &'a str) -> Self {
        Self { range, replacement }
    }

    /// Returns the UTF-16 range in the base revision.
    #[must_use]
    pub const fn range(&self) -> &Range<usize> {
        &self.range
    }

    /// Returns the replacement UTF-8 text.
    #[must_use]
    pub const fn replacement(&self) -> &'a str {
        self.replacement
    }
}

/// A move-only lease keeping one immutable source root alive.
///
/// Leases are `Send` so a serialized document actor may migrate between host
/// threads. They are deliberately `!Sync`: callers must not turn an immutable
/// source lease into an implicit cross-thread sharing boundary.
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<flark_engine::SourceSnapshotLease>();
/// ```
///
/// ```compile_fail
/// let store = flark_engine::SourceStore::new("linear").unwrap();
/// let lease = store.snapshot();
/// let duplicate = lease.clone();
/// ```
pub struct SourceSnapshotLease {
    document: SourceDocumentId,
    revision: SourceRevision,
    root: Arc<SourceRoot>,
    _not_sync: PhantomData<Cell<()>>,
}

/// Exact physical-line rank, byte coverage, and terminator resolved by one
/// immutable source lease.
///
/// This is a narrow rank/select receipt, not a view into the underlying rope.
/// The source stamp lets parser-owned callers reject accidental joins across
/// revisions without rescanning the prefix preceding the selected line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePhysicalLineLocation {
    source: SourceVersion,
    ordinal: usize,
    byte_range: Range<usize>,
    ending: LineEnding,
}

impl SourcePhysicalLineLocation {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn ordinal(&self) -> usize {
        self.ordinal
    }

    #[must_use]
    pub fn byte_range(&self) -> Range<usize> {
        self.byte_range.clone()
    }

    #[must_use]
    pub const fn ending(&self) -> LineEnding {
        self.ending
    }
}

impl fmt::Debug for SourceSnapshotLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceSnapshotLease")
            .field("version", &self.version())
            .finish_non_exhaustive()
    }
}

impl SourceSnapshotLease {
    pub(crate) fn duplicate(&self) -> Self {
        Self {
            document: self.document,
            revision: self.revision,
            root: Arc::clone(&self.root),
            _not_sync: PhantomData,
        }
    }

    /// Returns the leased source version.
    #[must_use]
    pub fn version(&self) -> SourceVersion {
        version_for(self.revision, &self.root)
    }

    /// Returns semantic continuity independent of this immutable root.
    #[must_use]
    pub const fn authority(&self) -> SourceAuthority {
        SourceAuthority::new(self.document, self.revision)
    }

    /// Maps a UTF-16 code-unit boundary to its exact UTF-8 byte offset.
    ///
    /// Out-of-range offsets and offsets between the surrogate halves of a
    /// non-BMP scalar are reported as errors rather than delegated to Crop's
    /// panicking conversion API.
    pub fn byte_offset_for_utf16(&self, offset: usize) -> Result<usize, SourceEditError> {
        checked_byte_offset_for_utf16(&self.root.rope, offset)
    }

    /// Maps one exact UTF-8 scalar boundary to its UTF-16 code-unit offset.
    pub fn utf16_offset_for_byte(&self, offset: usize) -> Result<usize, SourceEditError> {
        validate_range(&self.root.rope, &(offset..offset))?;
        Ok(self.root.rope.utf16_code_unit_of_byte(offset))
    }

    /// Returns whether one scalar-aligned cut begins a CommonMark physical
    /// line in this exact snapshot.
    ///
    /// A cut between the two bytes of CRLF is not a line start. A cut after a
    /// bare CR, LF, or at byte zero is. This authenticates only the source
    /// boundary; parser state and physical-line ordinal remain parser-owned.
    pub fn is_physical_line_start(&self, offset: usize) -> Result<bool, SourceEditError> {
        validate_range(&self.root.rope, &(offset..offset))?;
        if offset == 0 {
            return Ok(true);
        }
        match self.root.rope.byte(offset - 1) {
            b'\n' => Ok(true),
            b'\r' if offset == self.root.rope.byte_len() => Ok(true),
            b'\r' => Ok(self.root.rope.byte(offset) != b'\n'),
            _ => Ok(false),
        }
    }

    /// Resolves the physical line selected at one exact scalar-aligned byte
    /// boundary without scanning source bytes before that line.
    ///
    /// `Before` selects the byte immediately preceding an interior boundary;
    /// `After` selects the byte immediately following it. At BOF/EOF the
    /// affinity clamps to the first/last source byte, matching retained block
    /// point lookup. Empty source has no physical-line location.
    pub fn locate_physical_line(
        &self,
        byte_offset: usize,
        affinity: SourceBoundaryAffinity,
    ) -> Result<Option<SourcePhysicalLineLocation>, SourceEditError> {
        validate_range(&self.root.rope, &(byte_offset..byte_offset))?;
        let byte_len = self.root.rope.byte_len();
        if byte_len == 0 {
            return Ok(None);
        }
        let mut ordinal = self.root.rope.line_of_byte(byte_offset);
        if ordinal == self.root.rope.line_len()
            || (affinity == SourceBoundaryAffinity::Before
                && byte_offset > 0
                && self.root.rope.byte_of_line(ordinal) == byte_offset)
        {
            ordinal = ordinal.saturating_sub(1);
        }
        let start = self.root.rope.byte_of_line(ordinal);
        let end = self.root.rope.byte_of_line(ordinal + 1);
        let ending = match end.checked_sub(1).map(|tail| self.root.rope.byte(tail)) {
            Some(b'\n')
                if end.checked_sub(2).is_some_and(|before| {
                    before >= start && self.root.rope.byte(before) == b'\r'
                }) =>
            {
                LineEnding::CrLf
            }
            Some(b'\n') => LineEnding::Lf,
            Some(b'\r') => LineEnding::Cr,
            Some(_) | None => LineEnding::Eof,
        };
        Ok(Some(SourcePhysicalLineLocation {
            source: self.version(),
            ordinal,
            byte_range: start..end,
            ending,
        }))
    }

    /// Creates a cursor over the full source.
    pub fn cursor(self) -> Result<SourceCursor, SourceEditError> {
        let end = self.root.rope.byte_len();
        SourceCursor::new(self, 0..end)
    }

    /// Creates a cursor over a validated scalar-aligned byte range.
    pub fn cursor_in(self, range: Range<usize>) -> Result<SourceCursor, SourceEditError> {
        validate_range(&self.root.rope, &range)?;
        SourceCursor::new(self, range)
    }

    /// Creates a resumable physical-line cursor.
    pub fn lines(self) -> Result<PhysicalLineCursor, SourceEditError> {
        Ok(PhysicalLineCursor::new(self.cursor()?))
    }

    /// Consumes this lease into one linear discovery cursor plus one internal
    /// read baton. The aggregate never exposes either duplicate lease.
    pub fn line_cursor(self) -> Result<SourceLineCursor, SourceEditError> {
        let end = self.root.rope.byte_len();
        self.line_cursor_in(0..end)
    }

    /// Creates one linear physical-line cursor over a scalar-aligned range.
    ///
    /// Emitted [`LineDescriptor`] byte coordinates remain absolute within the
    /// complete source. This generic source operation validates only the
    /// selected range; it does not authenticate that either boundary is a
    /// parser restart or unchanged-lineage cut.
    pub fn line_cursor_in(self, range: Range<usize>) -> Result<SourceLineCursor, SourceEditError> {
        let version = self.version();
        let discovery = PhysicalLineCursor::new(self.duplicate().cursor_in(range.clone())?);
        Ok(SourceLineCursor {
            version,
            discovery,
            read_baton: self,
            allowed_range: range,
        })
    }
}

/// The one current source root for a serialized document actor.
///
/// This capability is `Send` but deliberately `!Sync`.
pub struct SourceStore {
    document: SourceDocumentId,
    revision: SourceRevision,
    root: Arc<SourceRoot>,
    _not_sync: PhantomData<Cell<()>>,
}

impl fmt::Debug for SourceStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceStore")
            .field("version", &self.version())
            .finish_non_exhaustive()
    }
}

impl SourceStore {
    /// Creates revision zero from UTF-8 source text.
    pub fn new(text: &str) -> Result<Self, SourceEditError> {
        let id = SourceRootId::allocate().ok_or(SourceEditError::IdentityExhausted)?;
        let document =
            SourceDocumentId::allocate().ok_or(SourceEditError::IdentityExhausted)?;
        Ok(Self {
            document,
            revision: SourceRevision::ZERO,
            root: Arc::new(SourceRoot {
                id,
                rope: Rope::from(text),
            }),
            _not_sync: PhantomData,
        })
    }

    /// Starts a paged, unpublished seed for an externally assigned revision.
    #[must_use]
    pub fn seed(revision: SourceRevision, expected_utf16_len: usize) -> SourceSeedBuilder {
        SourceSeedBuilder::new(revision, expected_utf16_len)
    }

    /// Returns the current source version.
    #[must_use]
    pub fn version(&self) -> SourceVersion {
        version_for(self.revision, &self.root)
    }

    /// Returns semantic continuity independent of the current immutable root.
    #[must_use]
    pub const fn authority(&self) -> SourceAuthority {
        SourceAuthority::new(self.document, self.revision)
    }

    /// Advances this read replica through one store-authenticated append-only
    /// opening transition without manufacturing an edit revision.
    #[cfg(feature = "progressive-source-probe")]
    pub fn adopt_opening_append(
        &mut self,
        proof: OpeningSourceAppendProof,
    ) -> Result<SourceAppendCommit, OpeningSourceError> {
        let (previous_opening, current_opening, snapshot) = proof.into_parts();
        let previous = source_version_for_opening(previous_opening);
        let current = source_version_for_opening(current_opening);
        if self.version() != previous || self.authority() != snapshot.authority() {
            return Err(OpeningSourceError::ForeignAuthority);
        }
        let source = snapshot.into_source_lease();
        if source.version() != current || source.authority() != self.authority() {
            return Err(OpeningSourceError::ForeignAuthority);
        }
        let retired_root = std::mem::replace(&mut self.root, source.root);
        Ok(SourceAppendCommit {
            receipt: SourceAppendReceipt {
                authority: self.authority(),
                previous,
                current,
                previous_generation: previous_opening.generation,
                current_generation: current_opening.generation,
                unchanged_prefix_bytes: previous.byte_len,
                unchanged_prefix_utf16: previous.utf16_len,
            },
            retired: SourceSnapshotLease {
                document: self.document,
                revision: self.revision,
                root: retired_root,
                _not_sync: PhantomData,
            },
        })
    }

    /// Acquires an immutable lease on the current root.
    #[must_use]
    pub fn snapshot(&self) -> SourceSnapshotLease {
        SourceSnapshotLease {
            document: self.document,
            revision: self.revision,
            root: Arc::clone(&self.root),
            _not_sync: PhantomData,
        }
    }

    /// Maps a UTF-16 code-unit boundary in the current source to UTF-8 bytes.
    pub fn byte_offset_for_utf16(&self, offset: usize) -> Result<usize, SourceEditError> {
        checked_byte_offset_for_utf16(&self.root.rope, offset)
    }

    /// Builds one atomic multi-operation source transition without publishing
    /// it. Every operation is expressed against `expected`, not against the
    /// output of the preceding operation.
    pub fn prepare_utf16_edit_intent(
        &self,
        expected: SourceVersion,
        declared_revision: SourceRevision,
        operations: &[SourceUtf16Operation<'_>],
    ) -> Result<PreparedSourceEditIntent, SourceEditError> {
        let plan = self.plan_utf16_edit_intent(expected, declared_revision, operations)?;
        self.materialize_utf16_edit_intent(plan)
    }

    /// Validates one UTF-16 intent and computes exact target metrics without
    /// cloning Crop, constructing a target root, or allocating a root ID.
    pub fn plan_utf16_edit_intent<'a>(
        &self,
        expected: SourceVersion,
        declared_revision: SourceRevision,
        operations: &'a [SourceUtf16Operation<'a>],
    ) -> Result<PlannedSourceEditIntent<'a>, SourceEditError> {
        let actual = self.version();
        if expected != actual {
            return Err(SourceEditError::StaleVersion { expected, actual });
        }

        let next_revision = self
            .revision
            .checked_next()
            .ok_or(SourceEditError::IdentityExhausted)?;
        if declared_revision != next_revision {
            return Err(SourceEditError::InvalidRevisionTransition {
                current: self.revision,
                declared: declared_revision,
            });
        }
        if operations.is_empty() {
            return Err(SourceEditError::EmptyEditIntent);
        }
        if operations.len() > SOURCE_EDIT_MAX_OPERATIONS {
            return Err(SourceEditError::TooManyEditOperations {
                observed: operations.len(),
                limit: SOURCE_EDIT_MAX_OPERATIONS,
            });
        }

        let mut validated = Vec::new();
        validated.try_reserve_exact(operations.len())?;
        let mut lineage_spans = Vec::new();
        lineage_spans.try_reserve_exact(operations.len())?;
        let source_utf16_len = self.root.rope.utf16_len();
        let mut previous_range: Option<&Range<usize>> = None;
        let mut replacement_byte_len = 0_usize;
        let mut replacement_utf16_len = 0_usize;
        let mut replaced_byte_len = 0_usize;
        let mut replaced_utf16_len = 0_usize;

        for operation in operations {
            let range = operation.range();
            if range.start > range.end || range.end > source_utf16_len {
                return Err(SourceEditError::InvalidUtf16Range {
                    start: range.start,
                    end: range.end,
                    len: source_utf16_len,
                });
            }
            if let Some(previous) = previous_range {
                if range.start < previous.start
                    || (range.start == previous.start && range.end < previous.end)
                    || range.start < previous.end
                {
                    return Err(SourceEditError::InvalidOperationOrder {
                        previous_start: previous.start,
                        previous_end: previous.end,
                        start: range.start,
                        end: range.end,
                    });
                }
            }

            let start_byte = checked_byte_offset_for_utf16(&self.root.rope, range.start)?;
            let end_byte = checked_byte_offset_for_utf16(&self.root.rope, range.end)?;
            let operation_replacement_utf16 = operation.replacement().encode_utf16().count();
            let new_byte_start = start_byte
                .checked_sub(replaced_byte_len)
                .and_then(|offset| offset.checked_add(replacement_byte_len))
                .ok_or(SourceEditError::MetricOverflow)?;
            let new_byte_end = new_byte_start
                .checked_add(operation.replacement().len())
                .ok_or(SourceEditError::MetricOverflow)?;
            let new_utf16_start = range
                .start
                .checked_sub(replaced_utf16_len)
                .and_then(|offset| offset.checked_add(replacement_utf16_len))
                .ok_or(SourceEditError::MetricOverflow)?;
            let new_utf16_end = new_utf16_start
                .checked_add(operation_replacement_utf16)
                .ok_or(SourceEditError::MetricOverflow)?;
            lineage_spans.push(SourceEditLineageSpan {
                old_bytes: start_byte..end_byte,
                new_bytes: new_byte_start..new_byte_end,
                old_utf16: range.clone(),
                new_utf16: new_utf16_start..new_utf16_end,
            });
            replaced_byte_len = replaced_byte_len
                .checked_add(end_byte - start_byte)
                .ok_or(SourceEditError::MetricOverflow)?;
            replaced_utf16_len = replaced_utf16_len
                .checked_add(range.end - range.start)
                .ok_or(SourceEditError::MetricOverflow)?;
            replacement_byte_len = replacement_byte_len
                .checked_add(operation.replacement().len())
                .ok_or(SourceEditError::MetricOverflow)?;
            replacement_utf16_len = replacement_utf16_len
                .checked_add(operation_replacement_utf16)
                .ok_or(SourceEditError::MetricOverflow)?;
            if replacement_utf16_len > SOURCE_EDIT_MAX_REPLACEMENT_UTF16 {
                return Err(SourceEditError::EditReplacementTooLarge {
                    observed: replacement_utf16_len,
                    limit: SOURCE_EDIT_MAX_REPLACEMENT_UTF16,
                });
            }

            validated.push(ValidatedSourceOperation {
                byte_range: start_byte..end_byte,
                replacement: operation.replacement(),
            });
            previous_range = Some(range);
        }

        let target_byte_len = self
            .root
            .rope
            .byte_len()
            .checked_sub(replaced_byte_len)
            .and_then(|length| length.checked_add(replacement_byte_len))
            .ok_or(SourceEditError::MetricOverflow)?;
        let target_utf16_len = source_utf16_len
            .checked_sub(replaced_utf16_len)
            .and_then(|length| length.checked_add(replacement_utf16_len))
            .ok_or(SourceEditError::MetricOverflow)?;

        Ok(PlannedSourceEditIntent {
            expected,
            revision: declared_revision,
            operation_count: operations.len(),
            replacement_byte_len,
            replacement_utf16_len,
            target_byte_len,
            target_utf16_len,
            operations: validated,
            lineage_spans,
            _not_sync: PhantomData,
        })
    }

    /// Materializes a previously admitted UTF-16 plan into one unpublished
    /// Crop root. The plan is rechecked against current authority before work.
    pub fn materialize_utf16_edit_intent(
        &self,
        plan: PlannedSourceEditIntent<'_>,
    ) -> Result<PreparedSourceEditIntent, SourceEditError> {
        let actual = self.version();
        if plan.expected != actual {
            return Err(SourceEditError::StaleVersion {
                expected: plan.expected,
                actual,
            });
        }

        let mut rope = self.root.rope.clone();
        for operation in plan.operations.into_iter().rev() {
            rope.replace(operation.byte_range, operation.replacement);
        }
        if rope.byte_len() != plan.target_byte_len || rope.utf16_len() != plan.target_utf16_len {
            return Err(SourceEditError::MetricOverflow);
        }
        let id = SourceRootId::allocate().ok_or(SourceEditError::IdentityExhausted)?;
        let root = Arc::new(SourceRoot { id, rope });

        Ok(PreparedSourceEditIntent {
            expected: plan.expected,
            revision: plan.revision,
            operation_count: plan.operation_count,
            replacement_byte_len: plan.replacement_byte_len,
            replacement_utf16_len: plan.replacement_utf16_len,
            lineage_spans: plan.lineage_spans,
            root,
            _not_sync: PhantomData,
        })
    }

    /// Builds a replacement root without publishing it.
    pub fn prepare_edit(
        &self,
        expected: SourceVersion,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<PreparedSourceEdit, SourceEditError> {
        self.validate_edit(expected, &range)?;
        let revision = self
            .revision
            .checked_next()
            .ok_or(SourceEditError::IdentityExhausted)?;
        let old_utf16_start = self.root.rope.utf16_code_unit_of_byte(range.start);
        let old_utf16_end = self.root.rope.utf16_code_unit_of_byte(range.end);
        let replacement_utf16_len = replacement.encode_utf16().count();
        let new_byte_end = range
            .start
            .checked_add(replacement.len())
            .ok_or(SourceEditError::MetricOverflow)?;
        let new_utf16_end = old_utf16_start
            .checked_add(replacement_utf16_len)
            .ok_or(SourceEditError::MetricOverflow)?;
        let mut lineage_spans = Vec::new();
        lineage_spans.try_reserve_exact(1)?;
        lineage_spans.push(SourceEditLineageSpan {
            old_bytes: range.clone(),
            new_bytes: range.start..new_byte_end,
            old_utf16: old_utf16_start..old_utf16_end,
            new_utf16: old_utf16_start..new_utf16_end,
        });
        let id = SourceRootId::allocate().ok_or(SourceEditError::IdentityExhausted)?;
        let mut rope = self.root.rope.clone();
        rope.replace(range.clone(), replacement);
        let root = Arc::new(SourceRoot { id, rope });

        Ok(PreparedSourceEdit {
            expected,
            replaced_range: range,
            replacement_byte_len: replacement.len(),
            lineage_spans,
            revision,
            root,
            _not_sync: PhantomData,
        })
    }

    pub(crate) fn validate_edit(
        &self,
        expected: SourceVersion,
        range: &Range<usize>,
    ) -> Result<(), SourceEditError> {
        let actual = self.version();
        if expected != actual {
            return Err(SourceEditError::StaleVersion { expected, actual });
        }
        validate_range(&self.root.rope, range)
    }

    /// Atomically makes a prepared root current and returns the retired lease.
    pub fn commit_prepared_edit(
        &mut self,
        prepared: PreparedSourceEdit,
    ) -> Result<SourceCommit, SourceEditError> {
        let actual = self.version();
        if prepared.expected != actual {
            return Err(SourceEditError::StaleVersion {
                expected: prepared.expected,
                actual,
            });
        }

        let previous = actual;
        let retired_root = std::mem::replace(&mut self.root, prepared.root);
        self.revision = prepared.revision;
        let current = self.version();

        Ok(SourceCommit {
            lineage: SourceEditLineage::new(previous, current, prepared.lineage_spans),
            receipt: SourceEditReceipt {
                previous,
                current,
                replaced_range: prepared.replaced_range,
                replacement_byte_len: prepared.replacement_byte_len,
            },
            retired: SourceSnapshotLease {
                document: self.document,
                revision: previous.revision,
                root: retired_root,
                _not_sync: PhantomData,
            },
        })
    }

    /// Atomically publishes a prepared multi-operation UTF-16 edit intent.
    pub fn commit_prepared_utf16_edit_intent(
        &mut self,
        prepared: PreparedSourceEditIntent,
    ) -> Result<SourceEditIntentCommit, SourceEditError> {
        let actual = self.version();
        if prepared.expected != actual {
            return Err(SourceEditError::StaleVersion {
                expected: prepared.expected,
                actual,
            });
        }

        let previous = actual;
        let retired_root = std::mem::replace(&mut self.root, prepared.root);
        self.revision = prepared.revision;
        let current = self.version();

        Ok(SourceEditIntentCommit {
            lineage: SourceEditLineage::new(previous, current, prepared.lineage_spans),
            receipt: SourceEditIntentReceipt {
                previous,
                current,
                operation_count: prepared.operation_count,
                replacement_byte_len: prepared.replacement_byte_len,
                replacement_utf16_len: prepared.replacement_utf16_len,
            },
            retired: SourceSnapshotLease {
                document: self.document,
                revision: previous.revision,
                root: retired_root,
                _not_sync: PhantomData,
            },
        })
    }

    pub(crate) fn into_snapshot(self) -> SourceSnapshotLease {
        SourceSnapshotLease {
            document: self.document,
            revision: self.revision,
            root: self.root,
            _not_sync: PhantomData,
        }
    }
}

/// Exact receipt for one append-only read-replica transition.
#[cfg(feature = "progressive-source-probe")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceAppendReceipt {
    authority: SourceAuthority,
    previous: SourceVersion,
    current: SourceVersion,
    previous_generation: u64,
    current_generation: u64,
    unchanged_prefix_bytes: usize,
    unchanged_prefix_utf16: usize,
}

#[cfg(feature = "progressive-source-probe")]
impl SourceAppendReceipt {
    #[must_use]
    pub const fn authority(self) -> SourceAuthority {
        self.authority
    }

    #[must_use]
    pub const fn previous(self) -> SourceVersion {
        self.previous
    }

    #[must_use]
    pub const fn current(self) -> SourceVersion {
        self.current
    }

    #[must_use]
    pub const fn previous_generation(self) -> u64 {
        self.previous_generation
    }

    #[must_use]
    pub const fn current_generation(self) -> u64 {
        self.current_generation
    }

    #[must_use]
    pub const fn unchanged_prefix_bytes(self) -> usize {
        self.unchanged_prefix_bytes
    }

    #[must_use]
    pub const fn unchanged_prefix_utf16(self) -> usize {
        self.unchanged_prefix_utf16
    }
}

/// Ownership result of adopting one append-only opening snapshot.
#[cfg(feature = "progressive-source-probe")]
pub struct SourceAppendCommit {
    receipt: SourceAppendReceipt,
    retired: SourceSnapshotLease,
}

#[cfg(feature = "progressive-source-probe")]
impl SourceAppendCommit {
    #[must_use]
    pub const fn receipt(&self) -> SourceAppendReceipt {
        self.receipt
    }

    #[must_use]
    pub fn into_parts(self) -> (SourceAppendReceipt, SourceSnapshotLease) {
        (self.receipt, self.retired)
    }
}

/// A linear, unpublished source transition.
pub struct PreparedSourceEdit {
    expected: SourceVersion,
    replaced_range: Range<usize>,
    replacement_byte_len: usize,
    lineage_spans: Vec<SourceEditLineageSpan>,
    revision: SourceRevision,
    root: Arc<SourceRoot>,
    _not_sync: PhantomData<Cell<()>>,
}

impl fmt::Debug for PreparedSourceEdit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedSourceEdit")
            .field("expected", &self.expected)
            .field("replaced_range", &self.replaced_range)
            .field("replacement_byte_len", &self.replacement_byte_len)
            .field("lineage_spans", &self.lineage_spans)
            .field("revision", &self.revision)
            .field("root", &self.root.id)
            .finish()
    }
}

struct ValidatedSourceOperation<'a> {
    byte_range: Range<usize>,
    replacement: &'a str,
}

/// A validated, exact-metric UTF-16 transition that has not materialized Crop.
///
/// This plan borrows replacement text and is deliberately move-only. Runtimes
/// can enforce target-size and retirement admission from it before any target
/// root construction or root-identity allocation occurs.
pub struct PlannedSourceEditIntent<'a> {
    expected: SourceVersion,
    revision: SourceRevision,
    operation_count: usize,
    replacement_byte_len: usize,
    replacement_utf16_len: usize,
    target_byte_len: usize,
    target_utf16_len: usize,
    operations: Vec<ValidatedSourceOperation<'a>>,
    lineage_spans: Vec<SourceEditLineageSpan>,
    _not_sync: PhantomData<Cell<()>>,
}

impl fmt::Debug for PlannedSourceEditIntent<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlannedSourceEditIntent")
            .field("expected", &self.expected)
            .field("revision", &self.revision)
            .field("operation_count", &self.operation_count)
            .field("replacement_byte_len", &self.replacement_byte_len)
            .field("replacement_utf16_len", &self.replacement_utf16_len)
            .field("target_byte_len", &self.target_byte_len)
            .field("target_utf16_len", &self.target_utf16_len)
            .finish_non_exhaustive()
    }
}

impl PlannedSourceEditIntent<'_> {
    #[must_use]
    pub const fn expected(&self) -> SourceVersion {
        self.expected
    }

    #[must_use]
    pub const fn declared_revision(&self) -> SourceRevision {
        self.revision
    }

    #[must_use]
    pub const fn operation_count(&self) -> usize {
        self.operation_count
    }

    #[must_use]
    pub const fn replacement_byte_len(&self) -> usize {
        self.replacement_byte_len
    }

    #[must_use]
    pub const fn replacement_utf16_len(&self) -> usize {
        self.replacement_utf16_len
    }

    #[must_use]
    pub const fn target_byte_len(&self) -> usize {
        self.target_byte_len
    }

    #[must_use]
    pub const fn target_utf16_len(&self) -> usize {
        self.target_utf16_len
    }
}

/// A linear, unpublished multi-operation UTF-16 source transition.
pub struct PreparedSourceEditIntent {
    expected: SourceVersion,
    revision: SourceRevision,
    operation_count: usize,
    replacement_byte_len: usize,
    replacement_utf16_len: usize,
    lineage_spans: Vec<SourceEditLineageSpan>,
    root: Arc<SourceRoot>,
    _not_sync: PhantomData<Cell<()>>,
}

impl fmt::Debug for PreparedSourceEditIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedSourceEditIntent")
            .field("expected", &self.expected)
            .field("revision", &self.revision)
            .field("operation_count", &self.operation_count)
            .field("replacement_byte_len", &self.replacement_byte_len)
            .field("replacement_utf16_len", &self.replacement_utf16_len)
            .field("lineage_spans", &self.lineage_spans)
            .field("root", &self.root.id)
            .finish()
    }
}

/// Metadata for one committed, atomic UTF-16 edit intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceEditIntentReceipt {
    previous: SourceVersion,
    current: SourceVersion,
    operation_count: usize,
    replacement_byte_len: usize,
    replacement_utf16_len: usize,
}

impl SourceEditIntentReceipt {
    /// Returns the source version replaced by this intent.
    #[must_use]
    pub const fn previous(&self) -> SourceVersion {
        self.previous
    }

    /// Returns the source version installed by this intent.
    #[must_use]
    pub const fn current(&self) -> SourceVersion {
        self.current
    }

    /// Returns the number of atomically applied operations.
    #[must_use]
    pub const fn operation_count(&self) -> usize {
        self.operation_count
    }

    /// Returns the total replacement size in UTF-8 bytes.
    #[must_use]
    pub const fn replacement_byte_len(&self) -> usize {
        self.replacement_byte_len
    }

    /// Returns the total replacement size in UTF-16 code units.
    #[must_use]
    pub const fn replacement_utf16_len(&self) -> usize {
        self.replacement_utf16_len
    }
}

/// The result of committing one prepared UTF-16 source intent.
pub struct SourceEditIntentCommit {
    receipt: SourceEditIntentReceipt,
    retired: SourceSnapshotLease,
    lineage: SourceEditLineage,
}

impl SourceEditIntentCommit {
    /// Returns commit metadata.
    #[must_use]
    pub const fn receipt(&self) -> &SourceEditIntentReceipt {
        &self.receipt
    }

    /// Splits the metadata from the lease that must be retired.
    #[must_use]
    pub fn into_parts(self) -> (SourceEditIntentReceipt, SourceSnapshotLease) {
        (self.receipt, self.retired)
    }

    /// Splits metadata and retirement ownership from exact scalar edit
    /// lineage. Existing callers that do not need incrementality can continue
    /// to use [`Self::into_parts`].
    #[must_use]
    pub fn into_parts_with_lineage(
        self,
    ) -> (
        SourceEditIntentReceipt,
        SourceSnapshotLease,
        SourceEditLineage,
    ) {
        (self.receipt, self.retired, self.lineage)
    }
}

/// Metadata for a committed source edit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceEditReceipt {
    previous: SourceVersion,
    current: SourceVersion,
    replaced_range: Range<usize>,
    replacement_byte_len: usize,
}

impl SourceEditReceipt {
    #[must_use]
    pub const fn previous(&self) -> SourceVersion {
        self.previous
    }

    #[must_use]
    pub const fn current(&self) -> SourceVersion {
        self.current
    }

    #[must_use]
    pub const fn replaced_range(&self) -> &Range<usize> {
        &self.replaced_range
    }

    #[must_use]
    pub const fn replacement_byte_len(&self) -> usize {
        self.replacement_byte_len
    }
}

/// The result of committing a prepared source edit.
pub struct SourceCommit {
    receipt: SourceEditReceipt,
    retired: SourceSnapshotLease,
    lineage: SourceEditLineage,
}

impl SourceCommit {
    /// Returns commit metadata.
    #[must_use]
    pub const fn receipt(&self) -> &SourceEditReceipt {
        &self.receipt
    }

    /// Splits the metadata from the lease that must be retired.
    #[must_use]
    pub fn into_parts(self) -> (SourceEditReceipt, SourceSnapshotLease) {
        (self.receipt, self.retired)
    }

    /// Splits metadata and retirement ownership from exact scalar edit
    /// lineage. Existing callers that do not need incrementality can continue
    /// to use [`Self::into_parts`].
    #[must_use]
    pub fn into_parts_with_lineage(
        self,
    ) -> (SourceEditReceipt, SourceSnapshotLease, SourceEditLineage) {
        (self.receipt, self.retired, self.lineage)
    }
}

/// A bounded-copy byte cursor over one immutable source lease.
pub struct SourceCursor {
    lease: SourceSnapshotLease,
    range: Range<usize>,
    position: usize,
    window_start: usize,
    window: Vec<u8>,
    window_offset: usize,
    refill_count: usize,
    max_refill_bytes: usize,
}

impl fmt::Debug for SourceCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceCursor")
            .field("version", &self.lease.version())
            .field("range", &self.range)
            .field("position", &self.position)
            .field("refill_count", &self.refill_count)
            .field("max_refill_bytes", &self.max_refill_bytes)
            .finish_non_exhaustive()
    }
}

impl SourceCursor {
    fn new(lease: SourceSnapshotLease, range: Range<usize>) -> Result<Self, SourceEditError> {
        let mut window = Vec::new();
        window.try_reserve_exact(SOURCE_CURSOR_WINDOW_BYTES)?;
        Ok(Self {
            position: range.start,
            window_start: range.start,
            lease,
            range,
            window,
            window_offset: 0,
            refill_count: 0,
            max_refill_bytes: 0,
        })
    }

    /// Returns the absolute byte position of the next byte.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.position
    }

    /// Returns the exclusive end of this cursor's range.
    #[must_use]
    pub const fn end(&self) -> usize {
        self.range.end
    }

    /// Returns the number of source-window refills performed.
    #[must_use]
    pub const fn refill_count(&self) -> usize {
        self.refill_count
    }

    /// Returns the largest refill copy observed.
    #[must_use]
    pub const fn max_refill_bytes(&self) -> usize {
        self.max_refill_bytes
    }

    /// Copies bytes into `output`, advancing the cursor.
    pub fn read(&mut self, output: &mut [u8]) -> usize {
        let mut written = 0;
        while written < output.len() {
            let Some(byte) = self.next_byte() else {
                break;
            };
            output[written] = byte;
            written += 1;
        }
        written
    }

    /// Returns the unique source lease after consuming the selected range.
    ///
    /// # Errors
    ///
    /// Returns [`SourceEditError::IncompleteCursor`] if unread selected bytes
    /// remain.
    pub fn finish(self) -> Result<SourceSnapshotLease, SourceEditError> {
        if self.position != self.range.end {
            return Err(SourceEditError::IncompleteCursor {
                expected: self.range.end,
                actual: self.position,
            });
        }
        Ok(self.lease)
    }

    /// Cancels this selected read and returns the unique immutable lease.
    #[must_use]
    pub fn cancel(self) -> SourceSnapshotLease {
        self.lease
    }

    pub(crate) fn peek_byte(&mut self) -> Option<u8> {
        if self.position >= self.range.end {
            return None;
        }
        if !self.window_contains_position() {
            self.refill();
        }
        self.window.get(self.window_offset).copied()
    }

    pub(crate) fn next_byte(&mut self) -> Option<u8> {
        let byte = self.peek_byte()?;
        self.position += 1;
        self.window_offset += 1;
        Some(byte)
    }

    fn window_contains_position(&self) -> bool {
        self.position >= self.window_start
            && self.position < self.window_start.saturating_add(self.window.len())
            && self.window_offset < self.window.len()
    }

    fn refill(&mut self) {
        self.window.clear();
        self.window_offset = 0;
        self.window_start = self.position;
        if self.position >= self.range.end {
            return;
        }

        let mut end = self
            .position
            .saturating_add(SOURCE_CURSOR_WINDOW_BYTES)
            .min(self.range.end);
        while end > self.position && !self.lease.root.rope.is_char_boundary(end) {
            end -= 1;
        }
        debug_assert!(end > self.position);

        let slice = self.lease.root.rope.byte_slice(self.position..end);
        for chunk in slice.chunks() {
            self.window.extend_from_slice(chunk.as_bytes());
        }
        self.refill_count += 1;
        self.max_refill_bytes = self.max_refill_bytes.max(self.window.len());
    }
}

/// The physical source newline terminating one line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    Lf,
    CrLf,
    Cr,
    Eof,
}

/// Byte and UTF-16 authority for one physical source line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineDescriptor {
    start_byte: usize,
    content_end_byte: usize,
    end_byte: usize,
    content_utf16: usize,
    physical_utf16: usize,
    ending: LineEnding,
}

impl LineDescriptor {
    #[must_use]
    pub const fn start_byte(self) -> usize {
        self.start_byte
    }

    #[must_use]
    pub const fn content_end_byte(self) -> usize {
        self.content_end_byte
    }

    #[must_use]
    pub const fn end_byte(self) -> usize {
        self.end_byte
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
    pub const fn ending(self) -> LineEnding {
        self.ending
    }
}

/// Result of one fuel-bounded physical-line scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinePoll {
    Pending,
    Line(LineDescriptor),
    Complete,
}

/// A resumable scanner that defines physical-line and UTF-16 accounting.
pub struct PhysicalLineCursor {
    cursor: SourceCursor,
    line_start: usize,
    content_utf16: usize,
    emitted_any: bool,
    pending_cr_content_end: Option<usize>,
    complete: bool,
}

/// One move-only aggregate for line discovery and exactly one line read baton.
///
/// Opening a line consumes the aggregate into [`SourceLineAccess`]. The baton
/// must be returned by finishing or cancelling that access before discovery
/// can continue, so callers cannot retain per-line source leases.
pub struct SourceLineCursor {
    version: SourceVersion,
    discovery: PhysicalLineCursor,
    read_baton: SourceSnapshotLease,
    allowed_range: Range<usize>,
}

impl fmt::Debug for SourceLineCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceLineCursor")
            .field("version", &self.version)
            .field("allowed_range", &self.allowed_range)
            .finish_non_exhaustive()
    }
}

impl SourceLineCursor {
    #[must_use]
    pub const fn version(&self) -> SourceVersion {
        self.version
    }

    /// Advances physical-line discovery by at most `fuel` source bytes.
    #[must_use]
    pub fn poll(&mut self, fuel: usize) -> LinePoll {
        self.discovery.poll(fuel)
    }

    /// Moves the only read baton into one scalar-aligned physical-line range.
    pub fn begin_access(self, range: Range<usize>) -> Result<SourceLineAccess, SourceEditError> {
        if range.start > range.end
            || range.start < self.allowed_range.start
            || range.end > self.allowed_range.end
        {
            return Err(SourceEditError::OutsideCursorRange {
                start: range.start,
                end: range.end,
                allowed_start: self.allowed_range.start,
                allowed_end: self.allowed_range.end,
            });
        }
        let cursor = self.read_baton.cursor_in(range)?;
        Ok(SourceLineAccess {
            version: self.version,
            discovery: self.discovery,
            cursor,
            allowed_range: self.allowed_range,
        })
    }

    /// Stops physical-line discovery and returns the aggregate's unique read
    /// baton.
    ///
    /// The discovery cursor owns only an internal duplicate used to find line
    /// boundaries. Consuming the aggregate drops that duplicate and preserves
    /// the caller-supplied move-only lease for an authenticated continuation.
    #[must_use]
    pub fn cancel(self) -> SourceSnapshotLease {
        self.read_baton
    }
}

/// The one outstanding sequential physical-line read from a line cursor.
pub struct SourceLineAccess {
    version: SourceVersion,
    discovery: PhysicalLineCursor,
    cursor: SourceCursor,
    allowed_range: Range<usize>,
}

impl fmt::Debug for SourceLineAccess {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceLineAccess")
            .field("version", &self.version)
            .field("position", &self.cursor.position())
            .field("end", &self.cursor.end())
            .finish_non_exhaustive()
    }
}

impl SourceLineAccess {
    /// Copies next-sequential bytes from the one outstanding line range.
    pub fn read(&mut self, output: &mut [u8]) -> usize {
        self.cursor.read(output)
    }

    #[must_use]
    pub const fn refill_count(&self) -> usize {
        self.cursor.refill_count()
    }

    #[must_use]
    pub const fn max_refill_bytes(&self) -> usize {
        self.cursor.max_refill_bytes()
    }

    /// Returns the read baton so physical-line discovery may resume.
    #[must_use]
    pub fn finish(self) -> SourceLineCursor {
        SourceLineCursor {
            version: self.version,
            discovery: self.discovery,
            read_baton: self.cursor.lease,
            allowed_range: self.allowed_range,
        }
    }
}

impl PhysicalLineCursor {
    fn new(cursor: SourceCursor) -> Self {
        let line_start = cursor.position();
        Self {
            cursor,
            line_start,
            content_utf16: 0,
            emitted_any: false,
            pending_cr_content_end: None,
            complete: false,
        }
    }

    /// Advances by at most `fuel` source bytes.
    pub fn poll(&mut self, mut fuel: usize) -> LinePoll {
        if self.complete {
            return LinePoll::Complete;
        }
        if fuel == 0 {
            return LinePoll::Pending;
        }

        loop {
            if let Some(content_end) = self.pending_cr_content_end {
                if self.cursor.peek_byte() == Some(b'\n') {
                    if fuel == 0 {
                        return LinePoll::Pending;
                    }
                    let _ = self.cursor.next_byte();
                    return self.emit_line(content_end, LineEnding::CrLf, 2);
                }
                return self.emit_line(content_end, LineEnding::Cr, 1);
            }

            if self.cursor.position() == self.cursor.end() {
                if self.line_start < self.cursor.end() || !self.emitted_any {
                    return self.emit_line(self.cursor.end(), LineEnding::Eof, 0);
                }
                self.complete = true;
                return LinePoll::Complete;
            }

            if fuel == 0 {
                return LinePoll::Pending;
            }

            let position = self.cursor.position();
            let byte = self
                .cursor
                .next_byte()
                .expect("position below cursor end must have a byte");
            fuel -= 1;

            match byte {
                b'\n' => return self.emit_line(position, LineEnding::Lf, 1),
                b'\r' => self.pending_cr_content_end = Some(position),
                _ => self.content_utf16 += utf16_units_for_lead_byte(byte),
            }
        }
    }

    fn emit_line(
        &mut self,
        content_end_byte: usize,
        ending: LineEnding,
        ending_utf16: usize,
    ) -> LinePoll {
        let line = LineDescriptor {
            start_byte: self.line_start,
            content_end_byte,
            end_byte: self.cursor.position(),
            content_utf16: self.content_utf16,
            physical_utf16: self.content_utf16 + ending_utf16,
            ending,
        };
        self.line_start = self.cursor.position();
        self.content_utf16 = 0;
        self.pending_cr_content_end = None;
        self.emitted_any = true;
        LinePoll::Line(line)
    }
}

fn version_for(revision: SourceRevision, root: &SourceRoot) -> SourceVersion {
    SourceVersion {
        revision,
        root: root.id,
        byte_len: root.rope.byte_len(),
        utf16_len: root.rope.utf16_len(),
    }
}

#[cfg(feature = "progressive-source-probe")]
fn source_version_for_opening(opening: OpeningSourceVersion) -> SourceVersion {
    SourceVersion::from_authenticated_parts(
        opening.revision,
        opening.root,
        opening.current_bytes,
        opening.current_utf16,
    )
}

fn validate_range(rope: &Rope, range: &Range<usize>) -> Result<(), SourceEditError> {
    let len = rope.byte_len();
    if range.start > range.end || range.end > len {
        return Err(SourceEditError::InvalidRange {
            start: range.start,
            end: range.end,
            len,
        });
    }
    if !rope.is_char_boundary(range.start) {
        return Err(SourceEditError::SplitUtf8Scalar {
            offset: range.start,
        });
    }
    if !rope.is_char_boundary(range.end) {
        return Err(SourceEditError::SplitUtf8Scalar { offset: range.end });
    }
    Ok(())
}

fn checked_byte_offset_for_utf16(
    rope: &Rope,
    utf16_offset: usize,
) -> Result<usize, SourceEditError> {
    let utf16_len = rope.utf16_len();
    if utf16_offset > utf16_len {
        return Err(SourceEditError::InvalidUtf16Offset {
            offset: utf16_offset,
            len: utf16_len,
        });
    }
    if utf16_offset == 0 {
        return Ok(0);
    }
    if utf16_offset == utf16_len {
        return Ok(rope.byte_len());
    }

    // Crop's pinned `byte_of_utf16_code_unit` currently rounds a split
    // surrogate offset down, while its public contract permits a panic. Avoid
    // both behaviors by lower-bounding only over known UTF-8 scalar
    // boundaries and comparing Crop's non-panicking byte-to-UTF-16 metric.
    let mut low = 0_usize;
    let mut high = rope.byte_len();
    while high - low > 4 {
        let mut midpoint = low + (high - low) / 2;
        while midpoint > low && !rope.is_char_boundary(midpoint) {
            midpoint -= 1;
        }
        if midpoint == low {
            break;
        }

        let midpoint_utf16 = rope.utf16_code_unit_of_byte(midpoint);
        match midpoint_utf16.cmp(&utf16_offset) {
            std::cmp::Ordering::Less => low = midpoint,
            std::cmp::Ordering::Equal => return Ok(midpoint),
            std::cmp::Ordering::Greater => high = midpoint,
        }
    }

    let mut byte_offset = low;
    while byte_offset <= high {
        let candidate_utf16 = rope.utf16_code_unit_of_byte(byte_offset);
        match candidate_utf16.cmp(&utf16_offset) {
            std::cmp::Ordering::Equal => return Ok(byte_offset),
            std::cmp::Ordering::Greater => {
                return Err(SourceEditError::SplitUtf16Scalar {
                    offset: utf16_offset,
                });
            }
            std::cmp::Ordering::Less => {}
        }

        if byte_offset == high {
            break;
        }
        let width = utf8_scalar_width(rope.byte(byte_offset));
        byte_offset = byte_offset
            .checked_add(width)
            .ok_or(SourceEditError::MetricOverflow)?;
        if byte_offset > high {
            break;
        }
    }

    Err(SourceEditError::SplitUtf16Scalar {
        offset: utf16_offset,
    })
}

const fn utf8_scalar_width(lead: u8) -> usize {
    match lead {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xff => 4,
        _ => 1,
    }
}

const fn utf16_units_for_lead_byte(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte < 0xc0 {
        0
    } else if byte < 0xf0 {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_line_start_query_distinguishes_lf_bare_cr_and_split_crlf() {
        let store = SourceStore::new("αx\nb\r\nc\rd").expect("source");
        let lease = store.snapshot();

        assert_eq!(lease.is_physical_line_start(0), Ok(true));
        assert_eq!(
            lease.is_physical_line_start("α".len()),
            Ok(false),
            "a scalar boundary inside one physical line is not a restart"
        );
        assert_eq!(lease.is_physical_line_start("αx\n".len()), Ok(true));
        assert_eq!(
            lease.is_physical_line_start("αx\nb\r".len()),
            Ok(false),
            "the cut inside CRLF is not a physical-line start"
        );
        assert_eq!(lease.is_physical_line_start("αx\nb\r\n".len()), Ok(true));
        assert_eq!(lease.is_physical_line_start("αx\nb\r\nc\r".len()), Ok(true));
        assert_eq!(
            lease.is_physical_line_start(1),
            Err(SourceEditError::SplitUtf8Scalar { offset: 1 })
        );
    }

    #[test]
    fn physical_line_rank_select_is_exact_for_bom_unicode_crlf_and_affinity() {
        let first = "\u{feff}- α\r\n";
        let second = "- β\n";
        let text = format!("{first}{second}- γ");
        let store = SourceStore::new(&text).expect("source");
        let lease = store.snapshot();
        let source = lease.version();
        let second_start = first.len();
        let third_start = first.len() + second.len();

        let second_line = lease
            .locate_physical_line(second_start + 2, SourceBoundaryAffinity::After)
            .expect("location")
            .expect("nonempty source");
        assert_eq!(second_line.source(), source);
        assert_eq!(second_line.ordinal(), 1);
        assert_eq!(second_line.byte_range(), second_start..third_start);
        assert_eq!(second_line.ending(), LineEnding::Lf);

        let after_beta = second_start + "- β".len();
        let before_unicode_boundary = lease
            .locate_physical_line(after_beta, SourceBoundaryAffinity::Before)
            .expect("before Unicode scalar boundary")
            .expect("line");
        assert_eq!(before_unicode_boundary.ordinal(), 1);
        assert_eq!(
            before_unicode_boundary.byte_range(),
            second_start..third_start
        );

        let before_boundary = lease
            .locate_physical_line(second_start, SourceBoundaryAffinity::Before)
            .expect("before")
            .expect("line");
        assert_eq!(before_boundary.ordinal(), 0);
        assert_eq!(before_boundary.byte_range(), 0..second_start);
        assert_eq!(before_boundary.ending(), LineEnding::CrLf);

        let after_boundary = lease
            .locate_physical_line(second_start, SourceBoundaryAffinity::After)
            .expect("after")
            .expect("line");
        assert_eq!(after_boundary.ordinal(), 1);
        assert_eq!(after_boundary.byte_range(), second_start..third_start);
        assert_eq!(after_boundary.ending(), LineEnding::Lf);

        let crlf_middle = first.len() - 1;
        for affinity in [
            SourceBoundaryAffinity::Before,
            SourceBoundaryAffinity::After,
        ] {
            let line = lease
                .locate_physical_line(crlf_middle, affinity)
                .expect("CRLF")
                .expect("line");
            assert_eq!(line.ordinal(), 0);
            assert_eq!(line.byte_range(), 0..second_start);
        }
    }

    #[test]
    fn ranged_line_cursor_emits_absolute_unicode_crlf_without_scanning_prefix() {
        let prefix = "p".repeat(100_000);
        let text = format!("{prefix}α\r\nβ\n");
        let store = SourceStore::new(&text).expect("source");
        let version = store.version();
        let start = prefix.len();
        let mut cursor = store
            .snapshot()
            .line_cursor_in(start..version.byte_len())
            .expect("ranged line cursor");
        assert_eq!(cursor.version(), version);

        let first = match cursor.poll("α\r\n".len()) {
            LinePoll::Line(line) => line,
            _ => panic!("tail line must complete without spending prefix fuel"),
        };
        assert_eq!(first.start_byte(), start);
        assert_eq!(first.content_end_byte(), start + "α".len());
        assert_eq!(first.end_byte(), start + "α\r\n".len());
        assert_eq!(first.content_utf16(), 1);
        assert_eq!(first.physical_utf16(), 3);
        assert_eq!(first.ending(), LineEnding::CrLf);

        let first_end = first.end_byte();
        let mut access = cursor
            .begin_access(first.start_byte()..first_end)
            .expect("first absolute line access");
        let mut bytes = [0_u8; 4];
        assert_eq!(access.read(&mut bytes), bytes.len());
        assert_eq!(&bytes, "α\r\n".as_bytes());
        cursor = access.finish();
        assert_eq!(cursor.version(), version);

        let second = match cursor.poll("β\n".len()) {
            LinePoll::Line(line) => line,
            _ => panic!("second ranged line"),
        };
        assert_eq!(second.start_byte(), first_end);
        assert_eq!(second.content_end_byte(), first_end + "β".len());
        assert_eq!(second.end_byte(), version.byte_len());
        assert_eq!(second.content_utf16(), 1);
        assert_eq!(second.physical_utf16(), 2);
        assert_eq!(second.ending(), LineEnding::Lf);
    }

    #[test]
    fn ranged_line_cursor_validates_scalar_range_and_keeps_linear_batons() {
        let store = SourceStore::new("prefix\nα\n").expect("source");
        let version = store.version();
        let start = "prefix\n".len();
        assert!(matches!(
            store
                .snapshot()
                .line_cursor_in(start + 1..version.byte_len()),
            Err(SourceEditError::SplitUtf8Scalar { offset }) if offset == start + 1
        ));
        let escaped = store
            .snapshot()
            .line_cursor_in(start..version.byte_len())
            .expect("bounded line cursor")
            .begin_access(0..start);
        assert!(matches!(
            escaped,
            Err(SourceEditError::OutsideCursorRange {
                start: 0,
                end,
                allowed_start,
                allowed_end,
            }) if end == start
                && allowed_start == start
                && allowed_end == version.byte_len()
        ));

        assert_eq!(Arc::strong_count(&store.root), 1);
        let mut cursor = store
            .snapshot()
            .line_cursor_in(start..version.byte_len())
            .expect("valid ranged cursor");
        assert_eq!(
            Arc::strong_count(&store.root),
            3,
            "one discovery lease plus one read baton"
        );
        let line = match cursor.poll("α\n".len()) {
            LinePoll::Line(line) => line,
            _ => panic!("ranged line"),
        };
        let mut access = cursor
            .begin_access(line.start_byte()..line.end_byte())
            .expect("linear access");
        let mut bytes = [0_u8; 3];
        assert_eq!(access.read(&mut bytes), bytes.len());
        assert_eq!(&bytes, "α\n".as_bytes());
        cursor = access.finish();
        assert_eq!(cursor.version(), version);
        assert_eq!(Arc::strong_count(&store.root), 3);
        drop(cursor);
        assert_eq!(Arc::strong_count(&store.root), 1);
    }

    #[test]
    fn single_byte_commit_emits_exact_unicode_lineage_and_maps_suffix() {
        let mut source = SourceStore::new("a😀éZ").expect("source");
        let previous = source.version();
        let prepared = source
            .prepare_edit(previous, 1..5, "λ🙂")
            .expect("unicode edit");
        let (_receipt, retired, lineage) = source
            .commit_prepared_edit(prepared)
            .expect("commit")
            .into_parts_with_lineage();
        let current = source.version();

        assert_eq!(lineage.previous(), previous);
        assert_eq!(lineage.current(), current);
        assert_eq!(lineage.spans().len(), 1);
        let span = &lineage.spans()[0];
        assert_eq!(span.old_bytes(), &(1..5));
        assert_eq!(span.new_bytes(), &(1..7));
        assert_eq!(span.old_utf16(), &(1..3));
        assert_eq!(span.new_utf16(), &(1..4));
        assert_eq!(
            lineage.map_unchanged_byte_range(previous, current, 0..1),
            Ok(0..1)
        );
        assert_eq!(
            lineage.map_unchanged_byte_range(previous, current, 5..8),
            Ok(7..10)
        );
        assert_eq!(
            lineage.map_unchanged_utf16_range(previous, current, 3..5),
            Ok(4..6)
        );

        drop(retired);
    }

    #[test]
    fn multi_operation_lineage_orders_insert_replace_delete_and_final_insert() {
        let mut source = SourceStore::new("A😀bc末Z").expect("source");
        let previous = source.version();
        let operations = [
            SourceUtf16Operation::new(0..0, "<"),
            SourceUtf16Operation::new(0..0, "["),
            SourceUtf16Operation::new(1..3, "λ"),
            SourceUtf16Operation::new(4..5, ""),
            SourceUtf16Operation::new(7..7, "!"),
        ];
        let prepared = source
            .prepare_utf16_edit_intent(previous, SourceRevision::new(1), &operations)
            .expect("multi-operation edit");
        let (_receipt, retired, lineage) = source
            .commit_prepared_utf16_edit_intent(prepared)
            .expect("commit")
            .into_parts_with_lineage();
        let current = source.version();

        let spans = lineage.spans();
        assert_eq!(spans.len(), operations.len());
        assert_eq!(spans[0].old_bytes(), &(0..0));
        assert_eq!(spans[0].new_bytes(), &(0..1));
        assert_eq!(spans[1].old_bytes(), &(0..0));
        assert_eq!(spans[1].new_bytes(), &(1..2));
        assert_eq!(spans[2].old_bytes(), &(1..5));
        assert_eq!(spans[2].new_bytes(), &(3..5));
        assert_eq!(spans[2].old_utf16(), &(1..3));
        assert_eq!(spans[2].new_utf16(), &(3..4));
        assert_eq!(spans[3].old_bytes(), &(6..7));
        assert_eq!(spans[3].new_bytes(), &(6..6));
        assert_eq!(spans[3].old_utf16(), &(4..5));
        assert_eq!(spans[3].new_utf16(), &(5..5));
        assert_eq!(spans[4].old_bytes(), &(11..11));
        assert_eq!(spans[4].new_bytes(), &(10..11));
        assert_eq!(spans[4].old_utf16(), &(7..7));
        assert_eq!(spans[4].new_utf16(), &(7..8));

        assert_eq!(
            lineage.map_unchanged_byte_range(previous, current, 0..1),
            Ok(2..3)
        );
        assert_eq!(
            lineage.map_unchanged_byte_range(previous, current, 5..6),
            Ok(5..6)
        );
        assert_eq!(
            lineage.map_unchanged_byte_range(previous, current, 7..11),
            Ok(6..10)
        );
        assert_eq!(
            lineage.map_unchanged_utf16_range(previous, current, 5..7),
            Ok(5..7)
        );

        drop(retired);
    }

    #[test]
    fn unchanged_mapping_rejects_edits_and_ranges_crossing_insertions() {
        let mut inserted = SourceStore::new("abcd").expect("source");
        let inserted_previous = inserted.version();
        let prepared = inserted
            .prepare_edit(inserted_previous, 2..2, "XY")
            .expect("insertion");
        let (_receipt, retired, insertion) = inserted
            .commit_prepared_edit(prepared)
            .expect("commit insertion")
            .into_parts_with_lineage();
        let inserted_current = inserted.version();

        assert_eq!(
            insertion.map_unchanged_byte_range(inserted_previous, inserted_current, 1..3),
            Err(SourceEditLineageError::EditedByteRange {
                start: 1,
                end: 3,
                span_index: 0,
            })
        );
        assert_eq!(
            insertion.map_unchanged_byte_range(inserted_previous, inserted_current, 0..2),
            Ok(0..2)
        );
        assert_eq!(
            insertion.map_unchanged_byte_range(inserted_previous, inserted_current, 2..4),
            Ok(4..6)
        );
        assert_eq!(
            insertion.map_unchanged_utf16_range(inserted_previous, inserted_current, 1..3),
            Err(SourceEditLineageError::EditedUtf16Range {
                start: 1,
                end: 3,
                span_index: 0,
            })
        );
        assert_eq!(
            insertion.map_unchanged_byte_range(inserted_previous, inserted_current, 2..2),
            Err(SourceEditLineageError::EmptyByteRange { offset: 2 })
        );
        drop(retired);

        let mut deleted = SourceStore::new("ab😀cd").expect("source");
        let deleted_previous = deleted.version();
        let prepared = deleted
            .prepare_edit(deleted_previous, 2..6, "")
            .expect("deletion");
        let (_receipt, retired, deletion) = deleted
            .commit_prepared_edit(prepared)
            .expect("commit deletion")
            .into_parts_with_lineage();
        let deleted_current = deleted.version();
        assert_eq!(deletion.spans()[0].new_bytes(), &(2..2));
        assert_eq!(deletion.spans()[0].new_utf16(), &(2..2));
        assert_eq!(
            deletion.map_unchanged_byte_range(deleted_previous, deleted_current, 2..6),
            Err(SourceEditLineageError::EditedByteRange {
                start: 2,
                end: 6,
                span_index: 0,
            })
        );
        drop(retired);
    }

    #[test]
    fn boundary_affinity_maps_touching_edits_and_rejects_replaced_interiors() {
        let mut source = SourceStore::new("abcd").expect("source");
        let previous = source.version();
        let prepared = source
            .prepare_utf16_edit_intent(
                previous,
                SourceRevision::new(1),
                &[
                    SourceUtf16Operation::new(1..2, "X"),
                    SourceUtf16Operation::new(2..2, "Y"),
                    SourceUtf16Operation::new(2..3, "Z"),
                ],
            )
            .expect("touching edit cluster");
        let (_receipt, retired, lineage) = source
            .commit_prepared_utf16_edit_intent(prepared)
            .expect("commit")
            .into_parts_with_lineage();
        let current = source.version();

        assert_eq!(
            lineage.map_byte_boundary(previous, current, 2, SourceBoundaryAffinity::Before,),
            Ok(1)
        );
        assert_eq!(
            lineage.map_byte_boundary(previous, current, 2, SourceBoundaryAffinity::After,),
            Ok(4)
        );
        assert_eq!(
            lineage.map_utf16_boundary(previous, current, 2, SourceBoundaryAffinity::Before,),
            Ok(1)
        );
        assert_eq!(
            lineage.map_utf16_boundary(previous, current, 2, SourceBoundaryAffinity::After,),
            Ok(4)
        );
        assert_eq!(
            lineage.map_byte_boundary(previous, current, 0, SourceBoundaryAffinity::Before,),
            Ok(0)
        );
        assert_eq!(
            lineage.map_byte_boundary(previous, current, 4, SourceBoundaryAffinity::After,),
            Ok(5)
        );
        assert_eq!(
            lineage.map_byte_boundary(previous, current, 5, SourceBoundaryAffinity::After,),
            Err(SourceEditLineageError::InvalidByteBoundary { offset: 5, len: 4 })
        );
        drop(retired);

        let mut replaced = SourceStore::new("abcdef").expect("source");
        let replaced_previous = replaced.version();
        let prepared = replaced
            .prepare_edit(replaced_previous, 1..4, "Q")
            .expect("replacement");
        let (_receipt, retired, replacement) = replaced
            .commit_prepared_edit(prepared)
            .expect("commit")
            .into_parts_with_lineage();
        let replaced_current = replaced.version();
        assert_eq!(
            replacement.map_byte_boundary(
                replaced_previous,
                replaced_current,
                2,
                SourceBoundaryAffinity::Before,
            ),
            Err(SourceEditLineageError::EditedByteBoundary {
                offset: 2,
                span_index: 0,
            })
        );
        drop(retired);
    }

    #[test]
    fn lineage_rejects_crossed_revision_and_root_authority() {
        let mut source = SourceStore::new("same").expect("source");
        let previous = source.version();
        let prepared = source.prepare_edit(previous, 0..1, "S").expect("edit");
        let (_receipt, retired, lineage) = source
            .commit_prepared_edit(prepared)
            .expect("commit")
            .into_parts_with_lineage();
        let current = source.version();
        let foreign_root = SourceStore::new("same").expect("foreign").version();
        let crossed_revision = SourceVersion::from_authenticated_parts(
            SourceRevision::new(99),
            previous.root(),
            previous.byte_len(),
            previous.utf16_len(),
        );

        assert_eq!(
            lineage.map_unchanged_byte_range(foreign_root, current, 1..4),
            Err(SourceEditLineageError::PreviousVersionMismatch {
                expected: previous,
                actual: foreign_root,
            })
        );
        assert_eq!(
            lineage.map_unchanged_byte_range(crossed_revision, current, 1..4),
            Err(SourceEditLineageError::PreviousVersionMismatch {
                expected: previous,
                actual: crossed_revision,
            })
        );
        assert_eq!(
            lineage.map_unchanged_byte_range(previous, foreign_root, 1..4),
            Err(SourceEditLineageError::CurrentVersionMismatch {
                expected: current,
                actual: foreign_root,
            })
        );

        drop(retired);
    }

    #[test]
    fn lineage_retains_no_retired_source_root() {
        let mut source = SourceStore::new("old source").expect("source");
        let previous = source.version();
        let old_root = Arc::downgrade(&source.root);
        let prepared = source.prepare_edit(previous, 0..3, "new").expect("edit");
        let (_receipt, retired, lineage) = source
            .commit_prepared_edit(prepared)
            .expect("commit")
            .into_parts_with_lineage();

        assert!(old_root.upgrade().is_some(), "retirement owns the old root");
        drop(retired);
        assert!(
            old_root.upgrade().is_none(),
            "scalar lineage must not retain the old source root"
        );
        assert_eq!(lineage.previous(), previous);
        assert_eq!(lineage.current(), source.version());
    }
}
