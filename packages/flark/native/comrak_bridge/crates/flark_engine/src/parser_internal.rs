//! Narrow workspace-only seam for the exact parser.
//!
//! `flark-parser` consumes bounded source access, scratch admission,
//! recursive-Green storage, the live References journal, and atomic inline
//! capture validation. Candidate publication, snapshot transport, and
//! projection storage are deliberately not part of this boundary.

pub use crate::inline_projection::{
    M11InlineLinkValue, M11InlineProjectionCaptureValidator, M11InlineProjectionError,
    M11InlineProjectionFact, M11InlineProjectionKind, M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES,
    M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW,
    M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS,
    M11_INLINE_PROJECTION_FLAG_CODE_TRIM_ONE_SPACE,
};
pub use crate::parser_range::{
    M11ParserRangeCursor, M11ParserRangeError, M11ParserRangePoll, M11ParserRangeStatus,
    M11ParserSourceRangeAuthority, M11_PARSER_RANGE_MAX_POLL_BYTES,
};
pub use crate::parser_scratch::{
    M11ParserScratchAdmission, M11ParserScratchError, M11ParserScratchReleaseFailure,
};
pub use crate::recursive_green::{
    splice_m11_recursive_green_structural_with_spanning_exit_repairs_atomic,
    M11RecursiveGreenBuild, M11RecursiveGreenBuildStatus, M11RecursiveGreenCachedRowEditCapability,
    M11RecursiveGreenCachedRowEditable, M11RecursiveGreenCloseFacts, M11RecursiveGreenClosedChild,
    M11RecursiveGreenCoveragePart, M11RecursiveGreenError, M11RecursiveGreenEvent,
    M11RecursiveGreenFactTag, M11RecursiveGreenFrameFence, M11RecursiveGreenFrameId,
    M11RecursiveGreenFrameQueryError, M11RecursiveGreenKind, M11RecursiveGreenLocation,
    M11RecursiveGreenLogicalAction, M11RecursiveGreenLogicalPosition,
    M11RecursiveGreenLogicalRange, M11RecursiveGreenPoint, M11RecursiveGreenPropertyChunk,
    M11RecursiveGreenQueryReceipt, M11RecursiveGreenReclaimPoll, M11RecursiveGreenRenderableRow,
    M11RecursiveGreenRoot, M11RecursiveGreenRowEditCapability, M11RecursiveGreenRowQueryLimits,
    M11RecursiveGreenRowQueryOutcome, M11RecursiveGreenRowWindow, M11RecursiveGreenSliceBuild,
    M11RecursiveGreenSliceOpenFrameBase, M11RecursiveGreenSliceRoot, M11RecursiveGreenSourceMetric,
    M11RecursiveGreenSpanningExitRepair, M11RecursiveGreenStoragePageIdentity,
    M11RecursiveGreenStructuralBoundary, M11RecursiveGreenStructuralBoundaryTransactionReplica,
    M11RecursiveGreenStructuralSpliceRebase, M11RecursiveGreenStructuralSpliceReceipt,
    M11RecursiveGreenStructuralSpliceSelection, M11RecursiveGreenTerminalFragmentBarrierStatus,
    M11RecursiveGreenTerminalFragmentBinding, M11RecursiveGreenTerminalFragmentCursor,
    M11RecursiveGreenTerminalFragmentCursorPoll, M11RecursiveGreenTerminalFragmentCursorStatus,
    M11RecursiveGreenTerminalFragmentDisposition, M11RecursiveGreenTerminalFragmentIdentity,
    M11RecursiveGreenTerminalFragmentRange, M11RecursiveGreenTerminalFragmentRewrite,
    M11RecursiveGreenTerminalFragmentRewritePoll, M11RecursiveGreenTerminalFragmentRewriteWork,
};
pub use crate::reference_journal::{
    M11ReferenceJournal, M11ReferenceJournalAdoptionStatus, M11ReferenceJournalError,
    M11ReferenceJournalOccurrence, M11ReferenceJournalOccurrenceStart, M11ReferenceJournalRange,
    M11ReferenceJournalRangeReplacement, M11ReferenceJournalRangeReplacementStatus,
    M11ReferenceJournalRoot, M11ReferenceJournalStatus, M11ReferenceJournalUnchangedPrefixAdoption,
    M11ReferenceJournalValueKind,
};
pub use crate::reference_resolver::{
    M11ReferenceResolution, M11ReferenceResolver, M11ReferenceResolverError, M11ResolvedReference,
};
