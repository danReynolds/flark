//! Mechanically correspondent Comrak block ordering on Flark-owned value state.

pub mod checkpoint;
pub mod parser;
pub mod provenance;
pub mod reference_prefix;
pub mod refillable_line;
pub mod render;
pub mod source;
pub mod source_ledger;
mod table;
pub mod tree;

pub use parser::{
    CancellationReceipt, DIRECT_SEGMENTED_LINE_WINDOW_BYTES,
    DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES, DirectBlockKind, DirectClosedChild,
    DirectCommand, DirectCoveragePart, DirectExternalWork, DirectExternalWorkKind,
    DirectFenceCharacter, DirectFencedCodeBoundary, DirectFencedCodeCloseFacts,
    DirectFencedCodeFacts, DirectFinalFacts, DirectHeadingFacts, DirectItemFacts, DirectLineEnding,
    DirectListFacts, DirectLogicalAction, DirectOwner, DirectParagraphOutcome, DirectPartialTab,
    DirectPollReceipt, DirectPollStatus, DirectReferencePrefixCommitStatus,
    DirectReferencePrefixContext, DirectReferencePrefixRequest, DirectSourceLinePollError,
    DirectSourceLinePollReceipt, DirectSourceLinePollStatus, DirectSourceLineSource,
    DirectSourceLineWork, DirectTerminatorResolution, DirectUnsupported, DirectValueBlockParser,
    FuelledValueBlockParser, ParseError, ValueBlockParser, WorkBudget, WorkPollReceipt, WorkStatus,
    parse_document,
};
#[doc(hidden)]
pub use parser::{
    DIRECT_DURABLE_GRAMMAR_FRAME_BYTES, DIRECT_DURABLE_GRAMMAR_HEADER_BYTES,
    DIRECT_DURABLE_LINE_BOUNDARY_FRAME_BYTES, DIRECT_DURABLE_LINE_BOUNDARY_HEADER_BYTES,
    DirectDurableGrammarCapture, DirectDurableGrammarCaptureReceipt,
    DirectDurableGrammarFrameRecord, DirectDurableGrammarHeader, DirectDurableLineBoundaryCapture,
    DirectDurableLineBoundaryCaptureReceipt, DirectDurableLineBoundaryFrameRecord,
    DirectDurableLineBoundaryHeader, DirectGrammarContinuation, DirectLineBoundaryDeferredRole,
    DirectLineBoundaryPairingView, DirectLineBoundaryPause, DirectLineBoundaryPauseReceipt,
    DirectLineBoundaryResumeCursor, DirectRestartFrameOutput, DirectRestartLineLocalContinuation,
    DirectRestartLineLocalOutput, DirectRestartOutput,
};
pub use reference_prefix::{
    DIRECT_REFERENCE_LABEL_MAX_NORMALIZED_BYTES, DIRECT_REFERENCE_LABEL_MAX_RETAINED_BYTES,
    DirectReferenceDefinition, DirectReferenceLogicalPosition, DirectReferenceLogicalRange,
    DirectReferencePrefixDisposition, DirectReferencePrefixOutput, DirectReferencePrefixOutputAck,
    DirectReferencePrefixOutputAckStatus, DirectReferencePrefixPollError,
    DirectReferencePrefixPollReceipt, DirectReferencePrefixPollStatus, DirectReferencePrefixSource,
    DirectReferencePrefixTerminal, DirectReferencePrefixTerminalAck,
    DirectReferencePrefixTerminalOutput, DirectReferencePrefixWork, DirectReferenceValueTransform,
};
pub use render::{BoundedCodeInfo, CodeInfoTransformReceipt, bounded_code_info, normalized_html};
pub use source::{
    CoverageLeaf, CoverageRange, LeafContent, LogicalChunk, LogicalProjection,
    LogicalProjectionCursor, OriginRun, OriginTransform, ProjectionReadError, SourceBackedContent,
    SourceDocument,
};
pub use tree::{
    Alignment, BlockDocument, BlockEvent, BlockKind, BlockNode, BlockTree, ListData, ListDelimiter,
    ListType, LiteralOwnershipReceipt, NodeId, Position, ReferenceOccurrence, SyntaxProfile,
    TableData,
};
