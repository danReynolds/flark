//! Parser-independent ownership and source primitives for the Flark engine.
//!
//! This crate deliberately does not expose a public FFI surface or markdown
//! semantics. It owns the bounded, resumable substrate that later parser work
//! can build on without placing unbounded work on a UI caller.

#![forbid(unsafe_code)]

#[allow(dead_code)] // Typed transport is activated by the block-quote parser/host join.
mod block_quote_projection;
#[cfg_attr(not(feature = "parser-internal"), allow(dead_code))]
mod block_sequence;
#[allow(dead_code)] // Private until parser records and host decoding join the engine.
mod candidate_manifest;
mod document;
#[allow(dead_code)] // Private until the M1.1 parser endpoint owns publication offers.
mod host_store;
mod identity;
#[allow(dead_code)] // Typed transport is activated by the indented-code parser/host join.
mod indented_code_projection;
#[allow(dead_code)] // Sidecar primitives await generic closure transport extraction.
mod inline_overlay;
#[cfg_attr(not(feature = "parser-internal"), allow(dead_code))]
mod inline_projection;
#[doc(hidden)]
pub mod m11_host;
#[allow(dead_code)] // Activated by the M1.2 SourceFacts persistent-root cutover.
mod measured_sequence;
#[cfg_attr(not(feature = "parser-internal"), allow(dead_code))]
mod recursive_green;
#[cfg(feature = "parser-internal")]
mod reference_journal;
#[allow(dead_code)] // Private production substrate until the exact controller owns admission.
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
#[cfg_attr(not(feature = "parser-internal"), allow(dead_code))]
mod parser_pages;
#[cfg(feature = "parser-internal")]
mod parser_scratch;

pub use document::{
    ActiveCandidateInfo, DocumentRuntime, DocumentRuntimeConfig, DocumentRuntimeError,
    DocumentState, DrainPoll, EditReceipt, ExactUnchangedPrefixWitness,
    ExactUnchangedSuffixWitness, IncrementalSourceFactsPlan, ParsePlan, PersistentCertifiedSource,
    PersistentSourceFactsDeltaRootAuthority, PersistentSourceFactsDeltaScanWork,
    PersistentSourceFactsDeltaWitness, PersistentSourceFactsInfo, PersistentSourceFactsPageInfo,
    RuntimeSourceFactsPoll, Utf16EditReceipt,
};
pub use identity::{ArenaId, ArenaIdentity, CandidateGeneration, SourceRevision, SourceRootId};
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
