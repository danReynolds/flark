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

mod block_quote_projection;
mod block_sequence;
mod candidate_manifest;
mod document;
mod host_store;
mod identity;
mod indented_code_projection;
mod inline_overlay;
mod inline_projection;
#[doc(hidden)]
pub mod m11_host;
mod measured_sequence;
mod mersenne61;
mod recursive_green;
#[cfg(feature = "parser-internal")]
mod reference_journal;
mod reference_root;
mod source;
mod source_facts;
mod storage;

/// Narrow workspace-only parser/publication seam.
///
/// This feature is enabled by `flark-parser`; it is not part of the default
/// engine surface and deliberately exposes no arena identities.
#[cfg(feature = "parser-internal")]
#[doc(hidden)]
pub mod parser_internal;
mod parser_pages;
#[cfg(feature = "parser-internal")]
mod parser_scratch;

pub use document::{
    DocumentRuntime, DocumentRuntimeConfig, DocumentRuntimeError, DocumentState, DrainPoll,
    EditReceipt, ExactUnchangedPrefixWitness, ExactUnchangedSuffixWitness,
    IncrementalSourceFactsPlan, PersistentCertifiedSource, PersistentSourceFactsDeltaRootAuthority,
    PersistentSourceFactsDeltaScanWork, PersistentSourceFactsDeltaWitness,
    PersistentSourceFactsInfo, PersistentSourceFactsPageInfo, RuntimeSourceFactsPoll,
    Utf16EditReceipt,
};
#[cfg(feature = "progressive-source-probe")]
pub use identity::SourceLoadId;
pub use identity::{
    ArenaId, ArenaIdentity, CandidateGeneration, SourceAuthority, SourceDocumentId, SourceRevision,
    SourceRootId,
};
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
pub use source_facts::{
    CertifiedSource, ParserProfileId, PersistentSourceFactsWork, SourceContentFingerprint,
    SourceContentHash128, SourceFactCanonicalPage, SourceFactCheckpoint, SourceFactCheckpointPage,
    SourceFactPageDigest, SourceFactRelativeCheckpoint, SourceFactRootPage,
    SourceFactSegmentSummary, SourceFactSequenceDigest, SourceFactsAssemblyError,
    SourceFactsCompletion, SourceFactsCoverage, SourceFactsError, SourceFactsPoll, SourceFactsRoot,
    SourceFactsRootAdmission, SourceFactsRootBuilder, SourceFactsRootLimits, SourceFactsScanId,
    SourceFactsScanProfile, SourceFactsScanner, SourceFactsWork,
    PERSISTENT_SOURCE_FACTS_CHECKPOINT_ROOT_GUARD_ALGORITHM, SOURCE_CONTENT_FINGERPRINT_ALGORITHM,
    SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX, SOURCE_FACT_CHECKPOINT_SPACING_MAX_UTF16,
    SOURCE_FACT_ROOT_DEFAULT_MAX_CHECKPOINTS, SOURCE_FACT_ROOT_DEFAULT_MAX_PAGES,
    SOURCE_FACT_ROOT_DEFAULT_MAX_RESIDENT_BYTES, SOURCE_FACT_SOURCE_BYTES_PER_POLL_MAX,
};
pub use storage::{ArenaError, ArenaLimits, ArenaMetrics, ReclaimReceipt, ARENA_PAGE_BYTES};
