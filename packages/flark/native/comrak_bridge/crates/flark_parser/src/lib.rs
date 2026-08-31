//! Private exact-parser boundary for the Flark worker.
//!
//! The production block grammar is a mechanically promoted donor snapshot
//! driven through Flark-owned refillable source lines, fuelled control,
//! reference rendezvous, recursive-Green writing, and capture-only inline
//! projection.

#![forbid(unsafe_code)]

pub mod block_core;

mod contract;
mod edit_context;
mod gfm_inline_projection;
mod gfm_table_projection;
mod inline_autolink;
mod inline_bare_autolink;
mod inline_code;
mod inline_direct;
mod inline_edit_component;
mod inline_emphasis;
mod inline_hazard;
mod inline_lex;
mod inline_projection_job;
mod inline_radix;
mod parser_binding;
mod pending_presentation_plan;
mod persistent_recursive_green_session;
mod recursive_green_inline_leaf;
mod reference_label;
mod reference_value;
mod segmented_lexical;
mod source_adapter;

pub use contract::{
    M11ExactController, M11LineEnding, M11PhysicalLineFacts, M11SourceLinePollReceipt,
    M11SourceLinePollStatus, M11SourceLineSource, SourceLineIdentity,
};
pub use edit_context::{
    classify_m11_simple_edit_line, derive_m11_simple_block_prefix_plans,
    derive_m11_simple_block_transitions, M11SimpleBlockPrefixPlan, M11SimpleBlockTransition,
    M11SimpleBlockTransitionPresentation, M11SimpleEditLine, M11SimpleEditLineKind,
    M11SimpleEditListMarker, M11_SIMPLE_EDIT_LINE_MAX_BYTES,
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
pub use inline_edit_component::{
    M11InlineEditComponent, M11InlineEditComponentMatcher, M11_INLINE_EDIT_COMPONENTS_MAX,
    M11_INLINE_EDIT_COMPONENT_SOURCE_MAX_BYTES,
};
pub use inline_lex::{
    m11_is_markdown_punctuation, M11InlineLexError, M11InlineLexEvent, M11InlineLexEventKind,
    M11InlineLexHazardKind, M11InlineLexPoll, M11InlineLexPollStatus, M11InlineLexReceipt,
    M11InlineLexScanner, M11_INLINE_LEX_MAX_POLL_TRANSITIONS,
};
pub use inline_projection_job::{
    M11InlineProjectionCapture, M11InlineProjectionJob, M11InlineProjectionJobError,
    M11InlineProjectionJobPoll, M11InlineProjectionJobPollStatus,
    M11InlineProjectionJobReleasePoll, M11InlineProjectionOutcome,
    M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
};
pub use parser_binding::{M11ParserBinding, M11_GRAMMAR_REVISION};
pub use pending_presentation_plan::{
    derive_m11_pending_presentation_plan_seed, M11PendingPresentationPlanSeed,
    M11_PENDING_PRESENTATION_SEQUENCE_MAX_BYTES,
};
#[cfg(feature = "m11-compact-probe")]
pub use persistent_recursive_green_session::{
    build_m11_compact_first_viewport_probe, build_m11_progressive_compact_probe,
    build_m11_progressive_open_probe, M11CompactInlineProjectionProbe, M11CompactViewportProbe,
    M11CompactViewportProbeError, M11ProgressiveCompactProbe, M11ProgressiveOpenFeed,
    M11ProgressiveOpenSession, M11ProgressiveOpenSessionPoll,
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
pub use recursive_green_inline_leaf::{
    prepare_m11_recursive_green_slice_inline_leaf,
    prepare_m11_recursive_green_slice_inline_leaf_rows, M11RecursiveGreenInlineLeafPreparation,
    M11RecursiveGreenInlineLeafPreparationError, M11RecursiveGreenSliceInlineLeafRowBatch,
};
pub use source_adapter::{
    SnapshotLineCancellation, SnapshotLinePoll, SnapshotLineScanner, SnapshotLineSource,
    SnapshotPhysicalLine, SourceAdapterError,
};
