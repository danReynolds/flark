//! Non-aliasing identities for source, parse, and output lifetime domains.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRevision(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GrammarRevision(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParseGeneration(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRootId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemoteRootId(pub u64);

/// One exact accepted transition between immutable source snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceTransition {
    pub base_revision: SourceRevision,
    pub target_revision: SourceRevision,
    pub base_root: SourceRootId,
    pub result_root: SourceRootId,
}
