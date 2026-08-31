//! Immutable reference lookup capability for the live parser session.
//!
//! This is intentionally separate from candidate publication. A resolver is
//! valid only while its exact source revision and reference journal remain
//! current in the originating document runtime.

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use crate::candidate_manifest::CandidateAuthority;
use crate::document::DocumentRuntime;
use crate::identity::RuntimeIdentity;
use crate::reference_journal::{M11ReferenceJournalError, M11ReferenceJournalRoot};
use crate::reference_root::{PersistentBytesView, ReferenceRootError, ReferenceWinnerIndex};
use crate::storage::ArenaError;
use crate::SourceVersion;

/// Cooked target authority for one exact reference-label winner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11ResolvedReference {
    definition_ordinal: u64,
    destination_source: Range<u64>,
    title_source: Option<Range<u64>>,
    cooked_destination: Box<str>,
    cooked_title: Option<Box<str>>,
}

/// Definitive result of one source-bound normalized-label lookup.
///
/// `ValueTooLarge` is distinct from `Missing`: a real reference whose cooked
/// payload exceeds the bounded sidecar envelope must fail closed rather than
/// be reclassified as literal text. `Unknown` is used by a committed-prefix
/// resolver whose authority has not reached EOF.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum M11ReferenceResolution {
    Missing,
    Unknown,
    ValueTooLarge,
    Resolved(M11ResolvedReference),
}

impl M11ResolvedReference {
    /// Constructs parser-authenticated reference authority from an alternate
    /// exact winner index.
    #[doc(hidden)]
    #[must_use]
    pub fn new(
        definition_ordinal: u64,
        destination_source: Range<u64>,
        title_source: Option<Range<u64>>,
        cooked_destination: Box<str>,
        cooked_title: Option<Box<str>>,
    ) -> Self {
        Self {
            definition_ordinal,
            destination_source,
            title_source,
            cooked_destination,
            cooked_title,
        }
    }

    #[must_use]
    pub const fn definition_ordinal(&self) -> u64 {
        self.definition_ordinal
    }

    #[must_use]
    pub const fn destination_source(&self) -> &Range<u64> {
        &self.destination_source
    }

    #[must_use]
    pub const fn title_source(&self) -> Option<&Range<u64>> {
        self.title_source.as_ref()
    }

    #[must_use]
    pub const fn cooked_destination(&self) -> &str {
        &self.cooked_destination
    }

    #[must_use]
    pub fn cooked_title(&self) -> Option<&str> {
        self.cooked_title.as_deref()
    }
}

#[derive(Debug)]
enum ErrorInner {
    RuntimeMismatch,
    SourceAuthorityMismatch,
    InvalidData,
    AllocationFailed,
    Storage(ReferenceRootError),
}

/// Failure to use a live reference resolver under its exact source authority.
#[derive(Debug)]
pub struct M11ReferenceResolverError(ErrorInner);

impl fmt::Display for M11ReferenceResolverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ErrorInner::RuntimeMismatch => {
                formatter.write_str("reference resolver belongs to another document runtime")
            }
            ErrorInner::SourceAuthorityMismatch => {
                formatter.write_str("reference resolver source is no longer current")
            }
            ErrorInner::InvalidData => formatter.write_str("invalid reference resolver data"),
            ErrorInner::AllocationFailed => {
                formatter.write_str("reference resolver allocation failed")
            }
            ErrorInner::Storage(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11ReferenceResolverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            ErrorInner::Storage(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ReferenceRootError> for M11ReferenceResolverError {
    fn from(error: ReferenceRootError) -> Self {
        match error {
            ReferenceRootError::Arena(ArenaError::AllocationFailed) => {
                Self(ErrorInner::AllocationFailed)
            }
            error => Self(ErrorInner::Storage(error)),
        }
    }
}

/// Cloneable lookup capability over the immutable first-winner index owned by
/// one live reference journal. It owns no arena pages.
#[derive(Clone)]
pub struct M11ReferenceResolver {
    runtime_identity: RuntimeIdentity,
    source: SourceVersion,
    index: Arc<ReferenceWinnerIndex>,
}

impl fmt::Debug for M11ReferenceResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ReferenceResolver")
            .field("source", &self.source)
            .field("root", &self.index.root())
            .finish_non_exhaustive()
    }
}

impl M11ReferenceResolver {
    /// Mints a cheap resolver from the parser's already-complete live journal.
    #[doc(hidden)]
    pub fn from_live_reference_journal(
        runtime: &DocumentRuntime,
        journal: &M11ReferenceJournalRoot,
    ) -> Result<Self, M11ReferenceJournalError> {
        let (runtime_identity, index) = journal.resolver_parts(runtime)?;
        Ok(Self {
            runtime_identity,
            source: journal.source(),
            index,
        })
    }

    /// Temporary compatibility bridge for the legacy retained-candidate
    /// transport. Live parser sessions use [`Self::from_live_reference_journal`].
    pub(crate) fn from_retained_candidate(
        runtime_identity: RuntimeIdentity,
        authority: CandidateAuthority,
        root: crate::ArenaId,
        index: Arc<ReferenceWinnerIndex>,
    ) -> Result<Self, M11ReferenceResolverError> {
        let byte_len = usize::try_from(authority.source_bytes)
            .map_err(|_| M11ReferenceResolverError(ErrorInner::InvalidData))?;
        let utf16_len = usize::try_from(authority.source_utf16)
            .map_err(|_| M11ReferenceResolverError(ErrorInner::InvalidData))?;
        if !index.is_bound_to(authority, root) {
            return Err(M11ReferenceResolverError(ErrorInner::InvalidData));
        }
        Ok(Self {
            runtime_identity,
            source: SourceVersion::from_authenticated_parts(
                authority.source_revision,
                authority.source_root,
                byte_len,
                utf16_len,
            ),
            index,
        })
    }

    /// Resolves one already-normalized exact label under the same current
    /// source revision that minted this capability.
    pub fn resolve(
        &self,
        runtime: &DocumentRuntime,
        normalized_label: &str,
        maximum_cooked_bytes: usize,
    ) -> Result<M11ReferenceResolution, M11ReferenceResolverError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ReferenceResolverError(ErrorInner::RuntimeMismatch));
        }
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11ReferenceResolverError(
                ErrorInner::SourceAuthorityMismatch,
            ));
        }
        let Some(winner) = self
            .index
            .winner(runtime.producer_arena(), normalized_label.as_bytes())?
        else {
            return Ok(M11ReferenceResolution::Missing);
        };
        let destination_len = usize::try_from(winner.cooked_destination.len())
            .map_err(|_| M11ReferenceResolverError(ErrorInner::InvalidData))?;
        let title_len = winner
            .cooked_title
            .as_ref()
            .map(|title| usize::try_from(title.len()))
            .transpose()
            .map_err(|_| M11ReferenceResolverError(ErrorInner::InvalidData))?
            .unwrap_or(0);
        let maximum_cooked_bytes = maximum_cooked_bytes.min(
            crate::inline_projection::M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES.saturating_sub(32),
        );
        if destination_len
            .checked_add(title_len)
            .is_none_or(|total| total > maximum_cooked_bytes)
        {
            return Ok(M11ReferenceResolution::ValueTooLarge);
        }
        let cooked_destination = read_reference_utf8(winner.cooked_destination)?;
        let cooked_title = winner.cooked_title.map(read_reference_utf8).transpose()?;
        Ok(M11ReferenceResolution::Resolved(M11ResolvedReference {
            definition_ordinal: winner.ordinal,
            destination_source: winner.destination_source.bytes,
            title_source: winner.title_source.map(|source| source.bytes),
            cooked_destination,
            cooked_title,
        }))
    }
}

fn read_reference_utf8(
    value: PersistentBytesView<'_>,
) -> Result<Box<str>, M11ReferenceResolverError> {
    let len = usize::try_from(value.len())
        .map_err(|_| M11ReferenceResolverError(ErrorInner::InvalidData))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| M11ReferenceResolverError(ErrorInner::AllocationFailed))?;
    bytes.resize(len, 0);
    if value.read(0, &mut bytes)? != len {
        return Err(M11ReferenceResolverError(ErrorInner::InvalidData));
    }
    String::from_utf8(bytes)
        .map(String::into_boxed_str)
        .map_err(|_| M11ReferenceResolverError(ErrorInner::InvalidData))
}
