//! Parser-independent ownership and source primitives for the Flark engine.
//!
//! This crate deliberately does not expose a public FFI surface or markdown
//! semantics. It owns the bounded, resumable substrate that later parser work
//! can build on without placing unbounded work on a UI caller.

#![forbid(unsafe_code)]
// The standalone engine profile intentionally omits the parser consumer of
// persistent construction APIs. Runtime builds enable `parser-internal` and
// enforce the crate's deny-level dead/unused lint boundary without exception.
#![cfg_attr(not(feature = "parser-internal"), allow(dead_code, unused_imports))]

mod document;
mod identity;
mod measured_sequence;
mod mersenne61;
mod parser_profile;
mod recursive_green;
mod reference_authority;
#[cfg(feature = "parser-internal")]
mod reference_journal;
#[cfg(feature = "parser-internal")]
mod reference_resolver;
mod reference_root;
mod source;
mod storage;

/// Narrow workspace-only parser seam.
///
/// This feature is enabled by `flark-parser`; it is not part of the default
/// engine surface and deliberately exposes no arena identities.
#[cfg(feature = "parser-internal")]
#[doc(hidden)]
pub mod parser_internal;
mod parser_range;
#[cfg(feature = "parser-internal")]
mod parser_scratch;

pub use document::{
    DocumentRuntime, DocumentRuntimeConfig, DocumentRuntimeError, DocumentState, DrainPoll,
    EditReceipt, ExactUnchangedPrefixWitness, ExactUnchangedSuffixWitness, Utf16EditReceipt,
};
#[cfg(feature = "progressive-source-probe")]
pub use identity::SourceLoadId;
pub use identity::{
    ArenaId, ArenaIdentity, SourceAuthority, SourceDocumentId, SourceRevision, SourceRootId,
};
pub use parser_profile::ParserProfileId;
pub use source::{
    LineDescriptor, LineEnding, LinePoll, PhysicalLineCursor, PlannedSourceEditIntent,
    PreparedSourceEdit, PreparedSourceEditIntent, SourceBoundaryAffinity, SourceCommit,
    SourceCursor, SourceEditError, SourceEditIntentCommit, SourceEditIntentReceipt,
    SourceEditLineage, SourceEditLineageError, SourceEditLineageSpan, SourceEditReceipt,
    SourceLineAccess, SourceLineCursor, SourcePhysicalLineLocation, SourceSeedBuilder,
    SourceSnapshotLease, SourceStore, SourceUtf16Operation, SourceVersion,
    SOURCE_CURSOR_WINDOW_BYTES, SOURCE_EDIT_MAX_OPERATIONS, SOURCE_EDIT_MAX_REPLACEMENT_UTF16,
    SOURCE_SEED_PAGE_MAX_UTF16,
};
#[cfg(feature = "progressive-source-probe")]
pub use source::{
    OpeningSourceAppendProof, OpeningSourceError, OpeningSourceSnapshot, OpeningSourceStore,
    OpeningSourceVersion, SourceAppendCommit, SourceAppendReceipt,
};
pub use storage::{ArenaError, ArenaLimits, ArenaMetrics, ReclaimReceipt, ARENA_PAGE_BYTES};
