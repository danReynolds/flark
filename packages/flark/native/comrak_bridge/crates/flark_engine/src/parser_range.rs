//! Exact bounded source access for parser-owned work.
//!
//! Markdown semantics stay in `flark-parser`. This module owns only an exact
//! source-range authority and resumable bounded-copy cursors over that range.

use std::fmt;
use std::ops::Range;

use crate::document::DocumentRuntime;
use crate::identity::RuntimeIdentity;
use crate::source::{SourceCursor, SourceEditError, SourceSnapshotLease, SourceVersion};
use crate::ARENA_PAGE_BYTES;

/// Maximum source bytes copied by one range-cursor poll.
pub const M11_PARSER_RANGE_MAX_POLL_BYTES: usize = ARENA_PAGE_BYTES;

fn validate_source_range_authority(
    runtime: &DocumentRuntime,
    lease: &SourceSnapshotLease,
    source_range: &Range<usize>,
) -> Result<(RuntimeIdentity, SourceVersion), M11ParserRangeError> {
    let source = lease.version();
    if runtime.current_source_version() != Some(source) {
        return Err(M11ParserRangeError::SourceAuthorityMismatch);
    }
    if source_range.start > source_range.end
        || source_range.end > source.byte_len()
        || u32::try_from(source_range.end).is_err()
        || lease.utf16_offset_for_byte(source_range.start).is_err()
        || lease.utf16_offset_for_byte(source_range.end).is_err()
    {
        return Err(M11ParserRangeError::InvalidRange);
    }
    Ok((runtime.producer_identity(), source))
}

/// Authority, lifecycle, resource, or bounded-copy failure.
#[derive(Debug)]
pub enum M11ParserRangeError {
    InvalidRange,
    SourceAuthorityMismatch,
    InvalidState,
    WrongRuntime,
    ZeroFuel,
    PollLimitExceeded,
    OutputTooLarge,
    CounterOverflow,
    Source(SourceEditError),
}

impl fmt::Display for M11ParserRangeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRange => formatter.write_str("parser source range is invalid"),
            Self::SourceAuthorityMismatch => {
                formatter.write_str("parser source authority is not current")
            }
            Self::InvalidState => formatter.write_str("parser source cursor is in the wrong state"),
            Self::WrongRuntime => {
                formatter.write_str("parser source authority belongs to another document runtime")
            }
            Self::ZeroFuel => formatter.write_str("parser source poll requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("parser source poll exceeds the bounded byte limit")
            }
            Self::OutputTooLarge => {
                formatter.write_str("parser source output exceeds the bounded copy limit")
            }
            Self::CounterOverflow => formatter.write_str("parser source counter overflow"),
            Self::Source(error) => write!(formatter, "parser source failure: {error}"),
        }
    }
}

impl std::error::Error for M11ParserRangeError {}

impl From<SourceEditError> for M11ParserRangeError {
    fn from(value: SourceEditError) -> Self {
        Self::Source(value)
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
    fn new(lease: SourceSnapshotLease, range: Range<usize>) -> Result<Self, M11ParserRangeError> {
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
    ) -> Result<M11ParserRangePoll, M11ParserRangeError> {
        if fuel == 0 {
            return Err(M11ParserRangeError::ZeroFuel);
        }
        if fuel > M11_PARSER_RANGE_MAX_POLL_BYTES {
            return Err(M11ParserRangeError::PollLimitExceeded);
        }
        if output.is_empty() || output.len() > M11_PARSER_RANGE_MAX_POLL_BYTES {
            return Err(M11ParserRangeError::OutputTooLarge);
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
            .ok_or(M11ParserRangeError::InvalidState)?;
        let bytes_read = cursor.read(&mut output[..limit]);
        let complete = cursor.position() == cursor.end();
        self.receipt.transitions = self
            .receipt
            .transitions
            .checked_add(bytes_read)
            .ok_or(M11ParserRangeError::CounterOverflow)?;
        self.receipt.bytes_read = self
            .receipt
            .bytes_read
            .checked_add(bytes_read)
            .ok_or(M11ParserRangeError::CounterOverflow)?;
        self.receipt.refill_count = cursor.refill_count();
        self.receipt.maximum_refill_bytes = cursor.max_refill_bytes();
        if complete {
            let lease = self
                .cursor
                .take()
                .ok_or(M11ParserRangeError::InvalidState)?
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
    ) -> Result<Self, M11ParserRangeError> {
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
    pub fn validate(&self, runtime: &DocumentRuntime) -> Result<(), M11ParserRangeError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ParserRangeError::WrongRuntime);
        }
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11ParserRangeError::SourceAuthorityMismatch);
        }
        Ok(())
    }

    pub fn cursor(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<M11ParserRangeCursor, M11ParserRangeError> {
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
