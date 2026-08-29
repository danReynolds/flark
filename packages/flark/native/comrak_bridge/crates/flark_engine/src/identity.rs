use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SOURCE_ROOT_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SOURCE_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);
#[cfg(feature = "progressive-source-probe")]
static NEXT_SOURCE_LOAD_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_ARENA_ID: AtomicU64 = AtomicU64::new(1);

/// A monotonically increasing source revision within one document.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceRevision(u64);

impl SourceRevision {
    pub(crate) const ZERO: Self = Self(0);

    /// Creates a revision from the document owner's externally assigned value.
    ///
    /// Revisions are opaque to the source replica except that edit intents must
    /// advance the current value by exactly one.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    pub(crate) fn checked_next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

/// Identifies one logical document across immutable source roots and edits.
///
/// A root identifies storage. A document identity identifies continuity. In
/// particular, progressive append generations mint new roots without changing
/// this identity, while user edits advance the paired [`SourceRevision`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceDocumentId(u64);

impl SourceDocumentId {
    pub(crate) fn allocate() -> Option<Self> {
        NEXT_SOURCE_DOCUMENT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .ok()
            .map(Self)
    }

    /// Returns the process-local logical document identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Stable parser/publication authority for one logical source revision.
///
/// This deliberately omits the immutable root and its current admitted
/// dimensions. Those belong to a readable snapshot, not to semantic
/// continuity across append-only loading generations.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceAuthority {
    document: SourceDocumentId,
    revision: SourceRevision,
}

impl SourceAuthority {
    pub(crate) const fn new(document: SourceDocumentId, revision: SourceRevision) -> Self {
        Self { document, revision }
    }

    /// Returns the logical document identity.
    #[must_use]
    pub const fn document(self) -> SourceDocumentId {
        self.document
    }

    /// Returns the user-edit revision within that document.
    #[must_use]
    pub const fn revision(self) -> SourceRevision {
        self.revision
    }
}

/// Identifies one immutable source root.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceRootId(u64);

impl SourceRootId {
    pub(crate) const fn from_wire(value: u64) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    pub(crate) fn allocate() -> Option<Self> {
        NEXT_SOURCE_ROOT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .ok()
            .map(Self)
    }

    /// Returns the numeric identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Identifies one progressive source-admission transaction across immutable
/// prefix roots and user-edit revisions.
///
/// This is deliberately distinct from [`SourceRootId`]. Every published
/// prefix is one immutable root, while all of those roots belong to the same
/// load transaction until it is sealed or abandoned.
#[cfg(feature = "progressive-source-probe")]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceLoadId(u64);

#[cfg(feature = "progressive-source-probe")]
impl SourceLoadId {
    pub(crate) fn allocate() -> Option<Self> {
        NEXT_SOURCE_LOAD_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .ok()
            .map(Self)
    }

    /// Returns the process-local load identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Identifies one parse-candidate attempt within a document.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CandidateGeneration(u64);

impl CandidateGeneration {
    pub(crate) const FIRST: Self = Self(1);

    pub(crate) const fn from_wire(value: u64) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    pub(crate) fn checked_next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }

    /// Returns the numeric generation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Identifies one arena instance so handles cannot cross arena boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArenaIdentity(u64);

impl ArenaIdentity {
    pub(crate) fn allocate() -> Option<Self> {
        NEXT_ARENA_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .ok()
            .map(Self)
    }

    /// Returns the numeric identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A generation-checked arena handle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArenaId {
    pub(crate) arena: ArenaIdentity,
    pub(crate) slot: u32,
    pub(crate) generation: u32,
}

impl ArenaId {
    /// Returns the owning arena identity.
    #[must_use]
    pub const fn arena(self) -> ArenaIdentity {
        self.arena
    }

    /// Returns the stable slot index.
    #[must_use]
    pub const fn slot(self) -> u32 {
        self.slot
    }

    /// Returns the slot generation.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}
