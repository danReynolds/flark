use flark_engine::SourceVersion;

/// Stable identity for one physical line of one immutable source snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceLineIdentity {
    source: SourceVersion,
    ordinal: u32,
    start_byte: u32,
    end_byte: u32,
}

impl SourceLineIdentity {
    pub(crate) const fn new(
        source: SourceVersion,
        ordinal: u32,
        start_byte: u32,
        end_byte: u32,
    ) -> Self {
        Self {
            source,
            ordinal,
            start_byte,
            end_byte,
        }
    }

    /// Returns the immutable source identity and dimensions.
    #[must_use]
    pub const fn source(self) -> SourceVersion {
        self.source
    }

    /// Returns the zero-based physical-line ordinal.
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }

    /// Returns the absolute first source byte in the line.
    #[must_use]
    pub const fn start_byte(self) -> u32 {
        self.start_byte
    }

    /// Returns the exclusive absolute source end of the line.
    #[must_use]
    pub const fn end_byte(self) -> u32 {
        self.end_byte
    }

    /// Returns the number of physical UTF-8 bytes in the line.
    #[must_use]
    pub const fn physical_bytes(self) -> u32 {
        self.end_byte - self.start_byte
    }
}

/// Exact physical terminator recognized by the source authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11LineEnding {
    Lf,
    CrLf,
    Cr,
    Eof,
}

/// Source-owned facts passed to the controller's commit join.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11PhysicalLineFacts {
    identity: SourceLineIdentity,
    content_bytes: u32,
    content_utf16: u32,
    physical_utf16: u32,
    ending: M11LineEnding,
}

impl M11PhysicalLineFacts {
    pub(crate) const fn new(
        identity: SourceLineIdentity,
        content_bytes: u32,
        content_utf16: u32,
        physical_utf16: u32,
        ending: M11LineEnding,
    ) -> Self {
        Self {
            identity,
            content_bytes,
            content_utf16,
            physical_utf16,
            ending,
        }
    }

    #[must_use]
    pub const fn identity(self) -> SourceLineIdentity {
        self.identity
    }

    #[must_use]
    pub const fn physical_bytes(self) -> u32 {
        self.identity.physical_bytes()
    }

    #[must_use]
    pub const fn content_bytes(self) -> u32 {
        self.content_bytes
    }

    #[must_use]
    pub const fn content_utf16(self) -> u32 {
        self.content_utf16
    }

    #[must_use]
    pub const fn physical_utf16(self) -> u32 {
        self.physical_utf16
    }

    #[must_use]
    pub const fn ending(self) -> M11LineEnding {
        self.ending
    }
}

/// Sequential, bounded physical-line source borrowed by the exact controller.
///
/// This is the production spelling of the proven donor contract. It does not
/// classify Markdown: a successful request must be the next unique physical
/// byte, and controller-owned repeated peeks never reach this boundary.
///
/// Provenance: `comrak_value_block_core/src/parser.rs` lines 473-490,
/// SHA-256 `73aeb1b5b33711afd6001da565f7297ed4426226de387d9f3185066cd17398ea`.
pub trait M11SourceLineSource {
    type Identity: Copy + Eq;
    type Error;

    fn identity(&self) -> Self::Identity;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn access_budget(&self) -> usize;

    /// Reads the next unique physical byte.
    ///
    /// # Errors
    ///
    /// Returns a source-owned error when identity, sequence, range, or the
    /// caller-issued access grant is violated.
    fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11SourceLinePollStatus {
    NeedMore,
    Matched,
}

/// Bounded-work receipt produced by one exact controller poll.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11SourceLinePollReceipt {
    pub status: M11SourceLinePollStatus,
    pub lexical_work_units: usize,
    pub source_first_reads: usize,
    pub physical_high_water: usize,
    pub retained_source_bytes: usize,
    pub source_budget_exhausted: bool,
    pub maximum_source_request_rewind_bytes: usize,
}

/// The already-proven lifecycle that a promoted M1.1 controller must expose.
///
/// This trait deliberately has no default implementation and no method that
/// accepts a preclassified Paragraph. Only the selected exact controller may
/// mint an admission or a terminal match.
pub trait M11ExactController<S>
where
    S: M11SourceLineSource<Identity = SourceLineIdentity>,
{
    /// Opaque, consuming authority for one in-flight physical line.
    type Admission;
    type Error;

    /// Mints one source-line admission at a controller-certified boundary.
    ///
    /// # Errors
    ///
    /// Returns a controller error when the boundary or physical dimensions
    /// cannot be admitted exactly.
    fn begin_source_line(
        &mut self,
        identity: SourceLineIdentity,
    ) -> Result<Self::Admission, Self::Error>;

    /// Advances the exact donor lifecycle within caller and source budgets.
    ///
    /// # Errors
    ///
    /// Returns a typed controller/source error. A failed admission must never
    /// fall through to another grammar implementation.
    fn poll_source_line(
        &mut self,
        admission: &mut Self::Admission,
        source: &mut S,
        fuel: usize,
    ) -> Result<M11SourceLinePollReceipt, Self::Error>;

    /// Consumes one terminal donor result and cross-checks source authority.
    ///
    /// # Errors
    ///
    /// Returns a controller error for incomplete, stale, crossed-source, or
    /// mismatched work before publication.
    fn commit_source_line(
        &mut self,
        admission: Self::Admission,
        facts: M11PhysicalLineFacts,
    ) -> Result<(), Self::Error>;

    /// Consumes and abandons suspended work without publishing grammar state.
    ///
    /// # Errors
    ///
    /// Returns a controller error if the admission does not belong to the
    /// active parser/source boundary.
    fn cancel_source_line(&mut self, admission: Self::Admission) -> Result<(), Self::Error>;
}
