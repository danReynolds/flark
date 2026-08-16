// SPDX-License-Identifier: BSD-2-Clause
// SPDX-FileCopyrightText: 2017-2026 Comrak contributors
// SPDX-FileCopyrightText: 2026 Flark contributors
//
// Mechanically adapted from the Comrak 0.54.0-correspondent value protocol in
// `tool/parser_research/comrak_value_block_core`. The pinned donor commit is
// 172c2ee7d2c5c262a28be3e407aadf705daea2b7. The complete license notice is in
// `vendor/comrak/COPYING`.

//! Storage-neutral commands emitted by Flark's correspondent block grammar.
//!
//! This module contains only scalar value state. It deliberately has no
//! source owner, persistent-storage identity, renderer, or scheduling policy.

mod child_fold;
mod command;
mod controller;
mod paragraph_fence;
mod reference_rendezvous;
mod writer;

pub use child_fold::{ChildSequenceFold, ClosedChild};
pub use command::{
    BlockCommand, BlockKind, BulletMarker, CoveragePart, FenceCharacter, FencedCodeBoundary,
    FencedCodeCloseFacts, FencedCodeFacts, FinalFacts, HeadingFacts, HeadingStyle, HtmlBlockFacts,
    HtmlBlockType, ItemFacts, LineEnding, LineSourcePosition, LineSourceRange, ListDelimiter,
    ListFacts, ListStyle, LogicalAction, ParagraphOutcome, PartialTab, SetextHeadingLevel,
    SourceMetric, StackOwner, TerminatorResolution,
};
#[cfg(any(test, feature = "m11-compact-probe"))]
pub(crate) use controller::M11DirectDurableBlockRestart;
pub use controller::{
    M11DirectBlockController, M11DirectBlockControllerError, M11DirectBlockDeferredRole,
    M11DirectBlockError, M11DirectBlockPollReceipt, M11DirectBlockPollStatus,
    M11DirectBlockRestart, M11DirectBlockUnsupported, M11DirectSourceLineAdmission,
    M11_DIRECT_BLOCK_MAX_LEXICAL_SLACK, M11_DIRECT_BLOCK_MAX_RETAINED_SOURCE_BYTES,
};
pub use paragraph_fence::{
    m11_block_quote_prefix_lineage, m11_recursive_green_row_presentation,
    resolve_m11_recursive_green_inline_leaf_fence,
    resolve_m11_recursive_green_inline_leaf_row_fence, resolve_m11_recursive_green_paragraph_fence,
    resolve_m11_recursive_green_slice_inline_leaf_row_fence, M11RecursiveGreenCodeBlockStyle,
    M11RecursiveGreenInlineLeafFence, M11RecursiveGreenInlineLeafKind, M11RecursiveGreenListMarker,
    M11RecursiveGreenParagraphFence, M11RecursiveGreenRowPresentation,
};
#[cfg(any(test, feature = "m11-compact-probe"))]
pub(crate) use reference_rendezvous::{
    M11CompactReferenceJournal, M11CompactReferenceReceipt, M11CompactReferenceResolver,
};
pub use reference_rendezvous::{
    M11ReferenceRendezvous, M11ReferenceRendezvousError, M11ReferenceRendezvousPoll,
    M11ReferenceRendezvousStatus,
};
pub(crate) use writer::{
    M11BlockCheckpointRebase, M11BlockOrdinaryCheckpointAdoption,
    M11BlockTerminalCheckpointAdoption, KIND_BLOCK_QUOTE, KIND_DOCUMENT, KIND_PARAGRAPH,
};
pub use writer::{
    M11BlockRestartCheckpoint, M11BlockRestartError, M11BlockStructuralAdoptionReceipt,
    M11BlockTerminalConvergenceCheckpoint, M11BlockWriter, M11BlockWriterError,
    M11BlockWriterOfferStatus, M11BlockWriterPoll, M11BlockWriterPollStatus,
};
#[cfg(any(test, feature = "m11-compact-probe"))]
pub(crate) use writer::{
    M11CompactProbeCheckpointFacts, M11CompactProbeFirstSlice, M11CompactProbeWriterReceipt,
};
