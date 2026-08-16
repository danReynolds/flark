//! Private exact-parser boundary for the Flark worker.
//!
//! The production block grammar is a mechanically promoted donor snapshot
//! driven through Flark-owned refillable source lines, fuelled control,
//! reference rendezvous, and recursive-Green writing. The older narrow clean
//! slices remain staged product contracts; they are not a second Markdown
//! classifier.

#![forbid(unsafe_code)]

pub mod block_core;

mod block_quote_projection_job;
mod bullet_list_local_delta;
mod bullet_list_projection_job;
mod contract;
mod edit_context;
mod exact_clean;
mod gfm_inline_projection;
mod gfm_table_projection;
mod indented_code_projection_job;
#[allow(dead_code)] // Staged angle-autolink stage for M11InlineProjectionJob.
mod inline_autolink;
#[allow(dead_code)] // Staged strict GFM bare-autolink stage.
mod inline_bare_autolink;
#[allow(dead_code)] // Staged code-span stage for M11InlineProjectionJob.
mod inline_code;
#[allow(dead_code)] // Staged direct-link/image stage for M11InlineProjectionJob.
mod inline_direct;
#[allow(dead_code)] // Staged emphasis stage for M11InlineProjectionJob.
mod inline_emphasis;
#[allow(dead_code)] // Staged precedence gate for M11InlineProjectionJob.
mod inline_hazard;
mod inline_lex;
mod inline_projection_job;
#[allow(dead_code)] // Staged storage foundation for M11InlineProjectionJob.
mod inline_radix;
mod persistent_recursive_green_session;
mod projected_inline_projection_job;
mod publication;
mod recursive_green_block_quote_projection;
mod recursive_green_paragraph_inline;
mod reference_cook;
mod reference_label;
mod reference_value;
mod segmented_lexical;
mod source_adapter;

pub use block_quote_projection_job::{
    M11BlockQuoteProjectionJob, M11BlockQuoteProjectionJobError, M11BlockQuoteProjectionJobPoll,
    M11BlockQuoteProjectionJobPollStatus, M11BlockQuoteProjectionJobReleasePoll,
    M11_BLOCK_QUOTE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
};
pub use bullet_list_local_delta::{
    M11BulletListLocalDeltaBoundaryFallback, M11BulletListLocalDeltaCancellation,
    M11BulletListLocalDeltaError, M11BulletListLocalDeltaJob, M11BulletListLocalDeltaPlan,
    M11BulletListLocalDeltaPoll, M11BulletListLocalDeltaResult, M11BulletListLocalDeltaTerminal,
    M11BulletListLocalDeltaWork, M11OrderedListLocalDeltaBoundaryFallback,
    M11OrderedListLocalDeltaCancellation, M11OrderedListLocalDeltaError,
    M11OrderedListLocalDeltaJob, M11OrderedListLocalDeltaPlan, M11OrderedListLocalDeltaPoll,
    M11OrderedListLocalDeltaResult, M11OrderedListLocalDeltaTerminal, M11OrderedListLocalDeltaWork,
    M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES, M11_ORDERED_LIST_LOCAL_DELTA_MAX_BYTES,
};
pub use bullet_list_projection_job::{
    M11BulletListItemProjectionJob, M11BulletListItemProjectionJobPoll,
    M11BulletListItemProjectionJobPollStatus, M11BulletListItemProjectionJobReleasePoll,
    M11BulletListItemProjectionOutput, M11BulletListProjectionJob, M11BulletListProjectionJobError,
    M11BulletListProjectionJobPoll, M11BulletListProjectionJobReleasePoll,
    M11OrderedListItemProjectionJob, M11OrderedListItemProjectionMetadata,
    M11_BULLET_LIST_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
};
pub use contract::{
    M11ExactController, M11LineEnding, M11PhysicalLineFacts, M11SourceLinePollReceipt,
    M11SourceLinePollStatus, M11SourceLineSource, SourceLineIdentity,
};
pub use edit_context::{
    classify_m11_simple_edit_line, M11SimpleEditLine, M11SimpleEditLineKind,
    M11SimpleEditListMarker, M11_SIMPLE_EDIT_LINE_MAX_BYTES,
};
pub use exact_clean::{
    LeadingReferencesAwaitingRemainder, LeadingReferencesCheckpointError,
    LeadingReferencesRestartCheckpoint, M11BlockQuoteDisposition, M11BlockQuoteLineKind,
    M11BlockQuoteLineMapping, M11BlockQuoteParagraphMapping, M11BlockQuoteUnsupportedReason,
    M11BulletListItemMapping, M11BulletListParagraphMapping, M11CleanBlockController,
    M11CleanControllerError, M11CleanControllerFault, M11CleanDocumentKind, M11CleanDocumentResult,
    M11CleanLeaf, M11CleanLineAdmission, M11ListUnsupportedReason, M11OrderedListItemMapping,
    M11OrderedListParagraphMapping, M11OrdinaryParagraphBofCropPlan,
    M11OrdinaryParagraphBofCropSelection, M11OrdinaryParagraphBoundaryCropPlanError,
    M11OrdinaryParagraphCheckpointError, M11OrdinaryParagraphCropPlan,
    M11OrdinaryParagraphCropPlanError, M11OrdinaryParagraphCropSelection,
    M11OrdinaryParagraphEofCropPlan, M11OrdinaryParagraphEofCropSelection,
    M11OrdinaryParagraphRestartCheckpoint, M11OrdinaryParagraphRestartCheckpoints,
    M11ParserBinding, M11ReferenceDefinition, M11UnknownReason, M11UnsupportedOpener,
    M11_GRAMMAR_REVISION, M11_ORDINARY_PARAGRAPH_CHECKPOINT_STRIDE_BYTES,
    M11_SEGMENTED_LINE_PREFIX_BYTES,
};
pub use gfm_inline_projection::{
    project_m11_gfm_inline, M11GfmInlineNode, M11GfmInlineOptions, M11GfmInlineProjectionError,
    M11GfmInlineReference, M11_GFM_INLINE_PROJECTION_MAX_BYTES,
};
pub use gfm_table_projection::{
    project_m11_gfm_table, M11GfmTableAlignment, M11GfmTableCell, M11GfmTableProjection,
    M11GfmTableProjectionError, M11GfmTableRow, M11_GFM_TABLE_MAX_BYTES, M11_GFM_TABLE_MAX_CELLS,
    M11_GFM_TABLE_MAX_COLUMNS, M11_GFM_TABLE_MAX_ROWS,
};
pub use indented_code_projection_job::{
    M11IndentedCodeProjectionJob, M11IndentedCodeProjectionJobError,
    M11IndentedCodeProjectionJobPoll, M11IndentedCodeProjectionJobPollStatus,
    M11IndentedCodeProjectionJobReleasePoll, M11_INDENTED_CODE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
};
pub use inline_lex::{
    M11InlineLexError, M11InlineLexEvent, M11InlineLexEventKind, M11InlineLexHazardKind,
    M11InlineLexPoll, M11InlineLexPollStatus, M11InlineLexReceipt, M11InlineLexScanner,
    M11_INLINE_LEX_MAX_POLL_TRANSITIONS,
};
pub use inline_projection_job::{
    M11InlineProjectionJob, M11InlineProjectionJobError, M11InlineProjectionJobPoll,
    M11InlineProjectionJobPollStatus, M11InlineProjectionJobReleasePoll, M11InlineProjectionOutput,
    M11InlineProjectionPublication, M11InlineProjectionPublicationParts,
    M11InlineProjectionUnsupportedRecord, M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
};
#[cfg(feature = "m11-compact-probe")]
pub use persistent_recursive_green_session::{
    build_m11_compact_first_viewport_probe, M11CompactInlineProjectionProbe,
    M11CompactViewportProbe, M11CompactViewportProbeError,
};
pub use persistent_recursive_green_session::{
    M11PersistentRecursiveGreenAdoption, M11PersistentRecursiveGreenAdoptionPoll,
    M11PersistentRecursiveGreenAdoptionStartFailure, M11PersistentRecursiveGreenAdoptionStatus,
    M11PersistentRecursiveGreenAdoptionWork, M11PersistentRecursiveGreenBuildPoll,
    M11PersistentRecursiveGreenBuildStatus, M11PersistentRecursiveGreenCleanBuild,
    M11PersistentRecursiveGreenCleanPlan, M11PersistentRecursiveGreenProjectionRegion,
    M11PersistentRecursiveGreenProjectionRegionKind, M11PersistentRecursiveGreenSession,
    M11PersistentRecursiveGreenSessionError, M11PersistentRecursiveGreenUpdate,
};
pub use projected_inline_projection_job::{
    M11ProjectedInlineProjectionJob, M11ProjectedInlineProjectionJobError,
    M11ProjectedInlineProjectionJobPoll, M11ProjectedInlineProjectionJobPollStatus,
    M11ProjectedInlineProjectionJobReleasePoll, M11ProjectedInlineProjectionOutput,
    M11_PROJECTED_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
};
pub use publication::{
    resolve_m11_published_block_quote_leaf_fence, resolve_m11_published_bullet_list_item_fences,
    resolve_m11_published_bullet_list_item_inline_fence,
    resolve_m11_published_bullet_list_leaf_fence, resolve_m11_published_indented_code_leaf_fence,
    resolve_m11_published_inline_leaf_fence, resolve_m11_published_inline_leaf_range,
    resolve_m11_published_ordered_list_item_fences, resolve_m11_published_ordered_list_leaf_fence,
    M11CandidateDerivationError, M11CandidateRoleBytes, M11CleanParseJob, M11CleanParseJobError,
    M11CleanParsePoll, M11ExactSegmentedCandidateInput, M11InlinePublicationError,
    M11LeadingReferencesCropError, M11LeadingReferencesCropParseJob, M11LeadingReferencesCropPoll,
    M11LeadingReferencesCropResult, M11LeadingReferencesCropWork,
    M11OrdinaryParagraphBofCropParseJob, M11OrdinaryParagraphBoundaryCropError,
    M11OrdinaryParagraphBoundaryCropPoll, M11OrdinaryParagraphBoundaryCropResult,
    M11OrdinaryParagraphBoundaryCropWork, M11OrdinaryParagraphCropError,
    M11OrdinaryParagraphCropParseJob, M11OrdinaryParagraphCropPoll, M11OrdinaryParagraphCropResult,
    M11OrdinaryParagraphCropWork, M11OrdinaryParagraphEofCropParseJob,
    M11OrdinaryParagraphRestartError, M11ParserCandidate, M11ParserCandidateWriter,
    M11ParserCandidateWriterPoll, M11ParserInlinePublication, M11ParserTerminalFacts,
    M11PublishedBlockQuoteLeafFence, M11PublishedBulletListItemInlineFence,
    M11PublishedBulletListItemInlineFenceOutcome, M11PublishedBulletListItemProjectionFence,
    M11PublishedBulletListLeafFence, M11PublishedBulletListTerminalEmpty,
    M11PublishedIndentedCodeLeafFence, M11PublishedInlineLeafFence,
    M11PublishedInlineLeafFenceResolution, M11PublishedInlineRangeBatch,
    M11PublishedInlineRangeEnd, M11PublishedInlineRangeError, M11PublishedInlineRangeLeafFence,
    M11PublishedInlineRangeLimits, M11PublishedOrderedListItemInlineFence,
    M11PublishedOrderedListItemInlineFenceOutcome, M11PublishedOrderedListItemProjectionFence,
    M11PublishedOrderedListLeafFence, M11PublishedOrderedListTerminalEmpty, M11_GREEN_RECORD_BYTES,
    M11_INLINE_FACTS_PER_PAGE, M11_INLINE_FACT_RECORD_BYTES, M11_INLINE_META_MAGIC,
    M11_INLINE_META_RECORD_BYTES, M11_INLINE_PAGE_HEADER_BYTES, M11_INLINE_PAGE_MAGIC,
    M11_INLINE_SCHEMA, M11_ORDINARY_CHECKPOINT_MERGE_RECORDS_PER_TRANSITION,
    M11_PROJECTION_RECORD_BYTES, M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES,
};
pub use recursive_green_block_quote_projection::{
    resolve_m11_recursive_green_block_quote_projection_fence,
    M11RecursiveGreenBlockQuoteProjectionFence, M11RecursiveGreenBlockQuoteProjectionPreparation,
};
pub use recursive_green_paragraph_inline::{
    prepare_m11_recursive_green_inline_leaf, prepare_m11_recursive_green_paragraph_inline,
    prepare_m11_recursive_green_slice_inline_leaf, M11RecursiveGreenInlineLeafPreparation,
    M11RecursiveGreenParagraphInlinePreparation, M11RecursiveGreenParagraphPreparationError,
    M11_RECURSIVE_GREEN_PARAGRAPH_BRIDGE_MAX_SOURCE_BYTES,
};
pub use reference_cook::{M11ReferenceCookReceipt, ReferenceCookError};
pub use source_adapter::{
    SnapshotLineCancellation, SnapshotLinePoll, SnapshotLineScanner, SnapshotLineSource,
    SnapshotPhysicalLine, SourceAdapterError,
};
