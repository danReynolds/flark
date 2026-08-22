use std::fmt;

use flark_engine::{
    LineDescriptor, LineEnding, LinePoll, SourceEditError, SourceLineAccess, SourceLineCursor,
    SourceSnapshotLease, SOURCE_CURSOR_WINDOW_BYTES,
};

use crate::contract::{
    M11LineEnding, M11PhysicalLineFacts, M11SourceLineSource, SourceLineIdentity,
};

/// Failure while binding or reading a bounded immutable source line.
#[derive(Debug, Eq, PartialEq)]
pub enum SourceAdapterError {
    Source(SourceEditError),
    MetricOverflow,
    OrdinalExhausted,
    OutstandingAccessBudget { remaining: usize },
    NonSequentialRead { expected: usize, actual: usize },
    AccessBudgetExhausted,
    PastEnd { offset: usize, len: usize },
    CursorDiverged,
}

impl fmt::Display for SourceAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "source adapter failed: {error}"),
            Self::MetricOverflow => formatter.write_str("source line exceeds the M1.1 u32 ABI"),
            Self::OrdinalExhausted => formatter.write_str("physical-line ordinal exhausted"),
            Self::OutstandingAccessBudget { remaining } => write!(
                formatter,
                "cannot replace an outstanding source grant of {remaining} bytes"
            ),
            Self::NonSequentialRead { expected, actual } => write!(
                formatter,
                "non-sequential source read: expected {expected}, received {actual}"
            ),
            Self::AccessBudgetExhausted => formatter.write_str("source access budget exhausted"),
            Self::PastEnd { offset, len } => {
                write!(
                    formatter,
                    "source read {offset} is outside physical line length {len}"
                )
            }
            Self::CursorDiverged => formatter.write_str("source cursor diverged from line facts"),
        }
    }
}

impl std::error::Error for SourceAdapterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SourceEditError> for SourceAdapterError {
    fn from(error: SourceEditError) -> Self {
        Self::Source(error)
    }
}

/// Fuel-bounded, snapshot-stamped physical-line discovery.
pub struct SnapshotLineScanner {
    cursor: SourceLineCursor,
    next_ordinal: u32,
    scanned_byte_position: usize,
}

impl SnapshotLineScanner {
    /// Binds a physical-line scanner to one immutable source root.
    ///
    /// # Errors
    ///
    /// Returns an error when the source cannot create a bounded cursor or its
    /// dimensions exceed the versioned M1.1 `u32` transport contract.
    pub fn new(lease: SourceSnapshotLease) -> Result<Self, SourceAdapterError> {
        Self::new_at(lease, 0, 0)
    }

    /// Binds a scanner at a parser-certified physical-line boundary.
    ///
    /// Callers must obtain `start_byte` and `next_ordinal` from one joined
    /// restart checkpoint; arbitrary offsets will be rejected later by line
    /// identity admission.
    #[doc(hidden)]
    pub fn new_at(
        lease: SourceSnapshotLease,
        start_byte: usize,
        next_ordinal: u32,
    ) -> Result<Self, SourceAdapterError> {
        let version = lease.version();
        Self::new_in(lease, start_byte..version.byte_len(), next_ordinal)
    }

    pub(crate) fn new_in(
        lease: SourceSnapshotLease,
        byte_range: std::ops::Range<usize>,
        next_ordinal: u32,
    ) -> Result<Self, SourceAdapterError> {
        let version = lease.version();
        let _ =
            u32::try_from(version.byte_len()).map_err(|_| SourceAdapterError::MetricOverflow)?;
        let _ =
            u32::try_from(version.utf16_len()).map_err(|_| SourceAdapterError::MetricOverflow)?;
        let start_byte = byte_range.start;
        let cursor = lease.line_cursor_in(byte_range)?;
        Ok(Self {
            cursor,
            next_ordinal,
            scanned_byte_position: start_byte,
        })
    }

    /// Advances physical-line discovery by at most `fuel` source bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if a line metric or ordinal cannot be represented by
    /// the M1.1 ABI.
    pub fn poll(self, fuel: usize) -> Result<SnapshotLinePoll, SourceAdapterError> {
        self.poll_counted(fuel).map(|(poll, _)| poll)
    }

    /// Internal work-accounted counterpart used by the aggregate parse
    /// quantum. The count is exactly the number of source bytes inspected by
    /// physical-line discovery in this call; zero-byte line/EOF boundaries
    /// remain the caller's explicit state-transition charge.
    pub(crate) fn poll_counted(
        self,
        fuel: usize,
    ) -> Result<(SnapshotLinePoll, usize), SourceAdapterError> {
        let (poll, inspected) = self.poll_counted_retaining_complete(fuel)?;
        let poll = match poll {
            SnapshotLineRetainedPoll::Pending(scanner) => SnapshotLinePoll::Pending(scanner),
            SnapshotLineRetainedPoll::Line(line) => SnapshotLinePoll::Line(line),
            SnapshotLineRetainedPoll::Complete(_) => SnapshotLinePoll::Complete,
        };
        Ok((poll, inspected))
    }

    pub(crate) fn poll_counted_retaining_complete(
        mut self,
        fuel: usize,
    ) -> Result<(SnapshotLineRetainedPoll, usize), SourceAdapterError> {
        let before = self.scanned_byte_position;
        match self.cursor.poll(fuel) {
            LinePoll::Pending => {
                self.scanned_byte_position = before
                    .checked_add(fuel)
                    .ok_or(SourceAdapterError::MetricOverflow)?;
                Ok((SnapshotLineRetainedPoll::Pending(self), fuel))
            }
            LinePoll::Complete => Ok((SnapshotLineRetainedPoll::Complete(self), 0)),
            LinePoll::Line(descriptor) => {
                let end = descriptor.end_byte();
                let inspected = end
                    .checked_sub(before)
                    .ok_or(SourceAdapterError::CursorDiverged)?;
                if inspected > fuel {
                    return Err(SourceAdapterError::CursorDiverged);
                }
                self.scanned_byte_position = end;
                let facts = line_facts(self.cursor.version(), self.next_ordinal, descriptor)?;
                self.next_ordinal = self
                    .next_ordinal
                    .checked_add(1)
                    .ok_or(SourceAdapterError::OrdinalExhausted)?;
                Ok((
                    SnapshotLineRetainedPoll::Line(SnapshotPhysicalLine {
                        scanner: self,
                        facts,
                    }),
                    inspected,
                ))
            }
        }
    }

    #[must_use]
    pub(crate) fn into_source_lease(self) -> SourceSnapshotLease {
        self.cursor.cancel()
    }

    #[cfg(any(test, feature = "m11-compact-probe"))]
    pub(crate) fn into_progressive_resume_parts(self) -> (SourceSnapshotLease, u32, usize) {
        (
            self.cursor.cancel(),
            self.next_ordinal,
            self.scanned_byte_position,
        )
    }
}

pub(crate) enum SnapshotLineRetainedPoll {
    Pending(SnapshotLineScanner),
    Line(SnapshotPhysicalLine),
    Complete(SnapshotLineScanner),
}

/// One result from fuel-bounded line discovery.
pub enum SnapshotLinePoll {
    Pending(SnapshotLineScanner),
    Line(SnapshotPhysicalLine),
    Complete,
}

/// Opaque source-stamped facts for one physical line.
pub struct SnapshotPhysicalLine {
    scanner: SnapshotLineScanner,
    facts: M11PhysicalLineFacts,
}

impl fmt::Debug for SnapshotPhysicalLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnapshotPhysicalLine")
            .field("facts", &self.facts)
            .finish_non_exhaustive()
    }
}

impl SnapshotPhysicalLine {
    #[must_use]
    pub const fn facts(&self) -> M11PhysicalLineFacts {
        self.facts
    }

    /// Returns the read baton without opening this line.
    #[must_use]
    pub fn skip(self) -> SnapshotLineScanner {
        self.scanner
    }

    /// Consumes the stamped line and creates its sequential source borrow.
    ///
    /// # Errors
    ///
    /// Returns an error if the immutable source can no longer honor its own
    /// scalar-aligned physical range.
    pub fn into_source(self) -> Result<SnapshotLineSource, SourceAdapterError> {
        SnapshotLineSource::new(self.scanner, self.facts)
    }
}

fn line_facts(
    version: flark_engine::SourceVersion,
    ordinal: u32,
    descriptor: LineDescriptor,
) -> Result<M11PhysicalLineFacts, SourceAdapterError> {
    let start_byte =
        u32::try_from(descriptor.start_byte()).map_err(|_| SourceAdapterError::MetricOverflow)?;
    let content_end_byte = u32::try_from(descriptor.content_end_byte())
        .map_err(|_| SourceAdapterError::MetricOverflow)?;
    let end_byte =
        u32::try_from(descriptor.end_byte()).map_err(|_| SourceAdapterError::MetricOverflow)?;
    let content_utf16 = u32::try_from(descriptor.content_utf16())
        .map_err(|_| SourceAdapterError::MetricOverflow)?;
    let physical_utf16 = u32::try_from(descriptor.physical_utf16())
        .map_err(|_| SourceAdapterError::MetricOverflow)?;
    let identity = SourceLineIdentity::new(version, ordinal, start_byte, end_byte);
    let ending = match descriptor.ending() {
        LineEnding::Lf => M11LineEnding::Lf,
        LineEnding::CrLf => M11LineEnding::CrLf,
        LineEnding::Cr => M11LineEnding::Cr,
        LineEnding::Eof => M11LineEnding::Eof,
    };
    Ok(M11PhysicalLineFacts::new(
        identity,
        content_end_byte
            .checked_sub(start_byte)
            .ok_or(SourceAdapterError::CursorDiverged)?,
        content_utf16,
        physical_utf16,
        ending,
    ))
}

/// Bounded sequential adapter over one source-stamped physical line.
pub struct SnapshotLineSource {
    facts: M11PhysicalLineFacts,
    access: SourceLineAccess,
    next_ordinal: u32,
    scanned_byte_position: usize,
    next_relative_offset: usize,
    access_budget: usize,
}

impl fmt::Debug for SnapshotLineSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnapshotLineSource")
            .field("facts", &self.facts)
            .field("next_relative_offset", &self.next_relative_offset)
            .field("access_budget", &self.access_budget)
            .field("refill_count", &self.access.refill_count())
            .field("max_refill_bytes", &self.access.max_refill_bytes())
            .finish_non_exhaustive()
    }
}

impl SnapshotLineSource {
    fn new(
        scanner: SnapshotLineScanner,
        facts: M11PhysicalLineFacts,
    ) -> Result<Self, SourceAdapterError> {
        let identity = facts.identity();
        let access = scanner.cursor.begin_access(
            usize::try_from(identity.start_byte())
                .map_err(|_| SourceAdapterError::MetricOverflow)?
                ..usize::try_from(identity.end_byte())
                    .map_err(|_| SourceAdapterError::MetricOverflow)?,
        )?;
        Ok(Self {
            facts,
            access,
            next_ordinal: scanner.next_ordinal,
            scanned_byte_position: scanner.scanned_byte_position,
            next_relative_offset: 0,
            access_budget: 0,
        })
    }

    #[must_use]
    pub const fn facts(&self) -> M11PhysicalLineFacts {
        self.facts
    }

    /// Grants at most one engine source window of next-sequential reads.
    ///
    /// # Errors
    ///
    /// Returns an error if the previous grant still has unused bytes. This
    /// prevents a scheduler from accidentally erasing source backpressure.
    pub fn replenish_access_budget(
        &mut self,
        requested: usize,
    ) -> Result<usize, SourceAdapterError> {
        if self.access_budget != 0 {
            return Err(SourceAdapterError::OutstandingAccessBudget {
                remaining: self.access_budget,
            });
        }
        let remaining = self.len().saturating_sub(self.next_relative_offset);
        let grant = requested.min(SOURCE_CURSOR_WINDOW_BYTES).min(remaining);
        self.access_budget = grant;
        Ok(grant)
    }

    #[must_use]
    pub const fn position(&self) -> usize {
        self.next_relative_offset
    }

    #[must_use]
    pub const fn refill_count(&self) -> usize {
        self.access.refill_count()
    }

    #[must_use]
    pub const fn max_refill_bytes(&self) -> usize {
        self.access.max_refill_bytes()
    }

    /// Consumes the source borrow without publishing grammar state.
    #[must_use]
    pub fn cancel(self) -> (SnapshotLineCancellation, SnapshotLineScanner) {
        let cancellation = SnapshotLineCancellation {
            identity: self.facts.identity(),
            bytes_read: self.next_relative_offset,
            unused_access_budget: self.access_budget,
        };
        let scanner = SnapshotLineScanner {
            cursor: self.access.finish(),
            next_ordinal: self.next_ordinal,
            scanned_byte_position: self.scanned_byte_position,
        };
        (cancellation, scanner)
    }

    /// Returns the read baton after completely consuming this physical line.
    ///
    /// # Errors
    ///
    /// Returns [`SourceAdapterError::CursorDiverged`] if the caller attempts
    /// to resume discovery before consuming the complete physical line.
    pub fn finish(self) -> Result<SnapshotLineScanner, SourceAdapterError> {
        if self.next_relative_offset != self.len() {
            return Err(SourceAdapterError::CursorDiverged);
        }
        Ok(SnapshotLineScanner {
            cursor: self.access.finish(),
            next_ordinal: self.next_ordinal,
            scanned_byte_position: self.scanned_byte_position,
        })
    }
}

impl M11SourceLineSource for SnapshotLineSource {
    type Identity = SourceLineIdentity;
    type Error = SourceAdapterError;

    fn identity(&self) -> Self::Identity {
        self.facts.identity()
    }

    fn len(&self) -> usize {
        usize::try_from(self.facts.physical_bytes()).expect("u32 fits usize")
    }

    fn access_budget(&self) -> usize {
        self.access_budget
    }

    // Adapter provenance: the sequential/budget invariants correspond to
    // `DirectSourceLineWork::poll_segmented_source_stage`, lines 2090-2160,
    // SHA-256 `5a3ca2aa820b5f5ee5a4fdb0443726f366bd5d398c060cbde24bbdf3ea874be1`.
    fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error> {
        if relative_offset != self.next_relative_offset {
            return Err(SourceAdapterError::NonSequentialRead {
                expected: self.next_relative_offset,
                actual: relative_offset,
            });
        }
        if self.access_budget == 0 {
            return Err(SourceAdapterError::AccessBudgetExhausted);
        }
        let len = self.len();
        if relative_offset >= len {
            return Err(SourceAdapterError::PastEnd {
                offset: relative_offset,
                len,
            });
        }
        let mut byte = [0_u8; 1];
        if self.access.read(&mut byte) != 1 {
            return Err(SourceAdapterError::CursorDiverged);
        }
        self.next_relative_offset += 1;
        self.access_budget -= 1;
        Ok(byte[0])
    }
}

/// Receipt proving how far a dropped admission consumed its immutable line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotLineCancellation {
    pub identity: SourceLineIdentity,
    pub bytes_read: usize,
    pub unused_access_budget: usize,
}

#[cfg(test)]
mod counted_poll_tests {
    use super::*;
    use flark_engine::SourceStore;

    fn consume(mut source: SnapshotLineSource) -> SnapshotLineSource {
        while source.position() < source.len() {
            let grant = source
                .replenish_access_budget(SOURCE_CURSOR_WINDOW_BYTES)
                .expect("bounded source grant");
            for _ in 0..grant {
                let offset = source.position();
                let _ = source.read_byte(offset).expect("sequential source byte");
            }
        }
        source
    }

    #[test]
    fn cr_boundary_emission_reports_only_newly_inspected_bytes() {
        let store = SourceStore::new("a\rX").expect("source");
        let scanner = SnapshotLineScanner::new(store.snapshot()).expect("scanner");
        let (poll, inspected) = scanner.poll_counted(2).expect("first discovery poll");
        assert_eq!(inspected, 2);
        let SnapshotLinePoll::Line(line) = poll else {
            panic!("non-LF lookahead emits the CR line without reading lookahead");
        };
        assert_eq!(line.facts().ending(), M11LineEnding::Cr);

        let source = consume(line.into_source().expect("first line source"));
        let scanner = source.finish().expect("finish returns discovery baton");
        let (poll, inspected) = scanner.poll_counted(1).expect("second line poll");
        assert_eq!(inspected, 1, "finish must retain discovery position");
        let SnapshotLinePoll::Line(line) = poll else {
            panic!("remaining EOF line must be discoverable");
        };
        assert_eq!(line.facts().content_bytes(), 1);
    }

    #[test]
    fn empty_eof_line_has_zero_inspected_bytes_for_explicit_boundary_charging() {
        let store = SourceStore::new("").expect("source");
        let scanner = SnapshotLineScanner::new(store.snapshot()).expect("scanner");
        let (poll, inspected) = scanner.poll_counted(1).expect("empty EOF poll");
        assert_eq!(inspected, 0);
        let SnapshotLinePoll::Line(line) = poll else {
            panic!("empty source still owns its EOF line");
        };
        assert_eq!(line.facts().physical_bytes(), 0);
        assert_eq!(line.facts().ending(), M11LineEnding::Eof);
    }

    #[test]
    fn cancellation_retains_the_counted_discovery_position() {
        let store = SourceStore::new("a\nb").expect("source");
        let scanner = SnapshotLineScanner::new(store.snapshot()).expect("scanner");
        let (poll, inspected) = scanner.poll_counted(2).expect("first line poll");
        assert_eq!(inspected, 2);
        let SnapshotLinePoll::Line(line) = poll else {
            panic!("first LF line");
        };
        let source = line.into_source().expect("first line source");
        let (cancellation, scanner) = source.cancel();
        assert_eq!(cancellation.bytes_read, 0);
        assert_eq!(cancellation.unused_access_budget, 0);

        let (poll, inspected) = scanner.poll_counted(1).expect("post-cancel line poll");
        assert_eq!(inspected, 1, "cancel must retain discovery position");
        let SnapshotLinePoll::Line(line) = poll else {
            panic!("remaining EOF line must be discoverable");
        };
        assert_eq!(line.facts().content_bytes(), 1);
    }
}
