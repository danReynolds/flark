use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SOURCE_ROOT_ID: AtomicU64 = AtomicU64::new(1);
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
