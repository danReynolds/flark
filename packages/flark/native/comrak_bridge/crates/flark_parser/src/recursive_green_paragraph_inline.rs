//! Bounded bridge from the direct block controller to capture-only inline
//! projection.
//!
//! This is intentionally a narrow checkpoint seam. It proves that the new
//! recursive Green authority can select an exact Paragraph and mint the
//! source capability consumed by `M11InlineProjectionJob`.

use std::{fmt, ops::Range};

use flark_engine::parser_internal::{
    M11RecursiveGreenFrameQueryError, M11RecursiveGreenFrameQueryLimits, M11RecursiveGreenPoint,
    M11RecursiveGreenQueryReceipt, M11RecursiveGreenRoot, M11RecursiveGreenRowQueryLimits,
    M11RecursiveGreenRowWindow, M11RecursiveGreenSliceRoot,
};
use flark_engine::{DocumentRuntime, DocumentRuntimeError, SOURCE_CURSOR_WINDOW_BYTES};

use crate::block_core::{
    resolve_m11_recursive_green_inline_leaf_fence,
    resolve_m11_recursive_green_slice_inline_leaf_row_fence,
    resolve_m11_recursive_green_slice_inline_leaf_row_fences, M11BlockWriter, M11BlockWriterError,
    M11BlockWriterOfferStatus, M11BlockWriterPollStatus, M11DirectBlockController,
    M11DirectBlockControllerError, M11DirectBlockError, M11DirectBlockPollStatus,
    M11DirectBlockUnsupported, M11RecursiveGreenInlineLeafFence, M11RecursiveGreenInlineLeafKind,
    M11RecursiveGreenParagraphFence,
};
use crate::{
    M11ExactController, M11SourceLinePollStatus, M11SourceLineSource, SnapshotLinePoll,
    SnapshotLineScanner, SnapshotLineSource, SourceAdapterError,
};

/// Temporary whole-document admission cap for the first product-shaped
/// recursive-Green Paragraph checkpoint.
///
/// Work still runs on the parser isolate/Worker. The cap prevents this
/// experimental bridge from becoming an accidental unbounded dispatch path
/// while incremental root installation is completed.
pub const M11_RECURSIVE_GREEN_PARAGRAPH_BRIDGE_MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAXIMUM_DRIVER_WORK: usize = 1_000_000;
const DRIVER_FUEL: usize = 64;
const RELEASE_FUEL: usize = 256;

#[derive(Debug)]
pub enum M11RecursiveGreenParagraphPreparationError {
    SourceTooLarge { observed: usize, maximum: usize },
    WorkBoundExceeded,
    NotParagraph,
    Document(DocumentRuntimeError),
    Source(SourceAdapterError),
    Controller(M11DirectBlockError),
    SourceController(M11DirectBlockControllerError<SourceAdapterError>),
    Writer(M11BlockWriterError),
    Query(M11RecursiveGreenFrameQueryError),
    InvalidState(&'static str),
}

impl fmt::Display for M11RecursiveGreenParagraphPreparationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLarge { observed, maximum } => write!(
                formatter,
                "recursive-Green Paragraph bridge source is {observed} bytes; maximum is {maximum}"
            ),
            Self::WorkBoundExceeded => {
                formatter.write_str("recursive-Green Paragraph bridge work bound exceeded")
            }
            Self::NotParagraph => {
                formatter.write_str("recursive-Green point is not owned by a final Paragraph")
            }
            Self::Document(error) => error.fmt(formatter),
            Self::Source(error) => error.fmt(formatter),
            Self::Controller(error) => write!(formatter, "direct block controller: {error:?}"),
            Self::SourceController(error) => {
                write!(formatter, "direct block source controller: {error:?}")
            }
            Self::Writer(error) => error.fmt(formatter),
            Self::Query(error) => error.fmt(formatter),
            Self::InvalidState(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for M11RecursiveGreenParagraphPreparationError {}

impl From<DocumentRuntimeError> for M11RecursiveGreenParagraphPreparationError {
    fn from(error: DocumentRuntimeError) -> Self {
        Self::Document(error)
    }
}

impl From<SourceAdapterError> for M11RecursiveGreenParagraphPreparationError {
    fn from(error: SourceAdapterError) -> Self {
        Self::Source(error)
    }
}

impl From<M11DirectBlockError> for M11RecursiveGreenParagraphPreparationError {
    fn from(error: M11DirectBlockError) -> Self {
        Self::Controller(error)
    }
}

impl From<M11DirectBlockControllerError<SourceAdapterError>>
    for M11RecursiveGreenParagraphPreparationError
{
    fn from(error: M11DirectBlockControllerError<SourceAdapterError>) -> Self {
        Self::SourceController(error)
    }
}

impl From<M11BlockWriterError> for M11RecursiveGreenParagraphPreparationError {
    fn from(error: M11BlockWriterError) -> Self {
        Self::Writer(error)
    }
}

impl From<M11RecursiveGreenFrameQueryError> for M11RecursiveGreenParagraphPreparationError {
    fn from(error: M11RecursiveGreenFrameQueryError) -> Self {
        Self::Query(error)
    }
}

/// Exact new-parser result ready to enter bounded inline capture. All ranges
/// and the move-only source capability come from the
/// recursive Green fence; job construction stays inside endpoint staging so
/// coalescing can discard an unstarted demand without violating job cleanup.
pub struct M11RecursiveGreenParagraphInlinePreparation {
    block_source: Range<u32>,
    block_source_utf16: Range<u32>,
    inline_source: Range<u32>,
    inline_source_utf16: Range<u32>,
    query_receipt: M11RecursiveGreenQueryReceipt,
    driver_work: usize,
    fence: M11RecursiveGreenParagraphFence,
}

/// Exact recursive-Green Paragraph-or-Heading result ready for the existing
/// credited inline sidecar.
pub struct M11RecursiveGreenInlineLeafPreparation {
    block_source: Range<u32>,
    block_source_utf16: Range<u32>,
    inline_source: Range<u32>,
    inline_source_utf16: Range<u32>,
    query_receipt: M11RecursiveGreenQueryReceipt,
    driver_work: usize,
    fence: M11RecursiveGreenInlineLeafFence,
}

impl M11RecursiveGreenInlineLeafPreparation {
    pub(crate) fn from_persistent_session(
        block_source: Range<u32>,
        block_source_utf16: Range<u32>,
        inline_source: Range<u32>,
        inline_source_utf16: Range<u32>,
        query_receipt: M11RecursiveGreenQueryReceipt,
        fence: M11RecursiveGreenInlineLeafFence,
    ) -> Self {
        Self {
            block_source,
            block_source_utf16,
            inline_source,
            inline_source_utf16,
            query_receipt,
            driver_work: 0,
            fence,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> M11RecursiveGreenInlineLeafKind {
        self.fence.kind()
    }

    #[must_use]
    pub fn block_source_range(&self) -> Range<u32> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> Range<u32> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub fn inline_source_range(&self) -> Range<u32> {
        self.inline_source.clone()
    }

    #[must_use]
    pub fn inline_source_utf16_range(&self) -> Range<u32> {
        self.inline_source_utf16.clone()
    }

    #[must_use]
    pub const fn query_receipt(&self) -> M11RecursiveGreenQueryReceipt {
        self.query_receipt
    }

    #[must_use]
    pub const fn driver_work(&self) -> usize {
        self.driver_work
    }

    #[must_use]
    pub fn into_fence(self) -> M11RecursiveGreenInlineLeafFence {
        self.fence
    }

    fn into_paragraph(
        self,
    ) -> Result<
        M11RecursiveGreenParagraphInlinePreparation,
        M11RecursiveGreenParagraphPreparationError,
    > {
        let fence = self
            .fence
            .into_paragraph()
            .ok_or(M11RecursiveGreenParagraphPreparationError::NotParagraph)?;
        Ok(M11RecursiveGreenParagraphInlinePreparation {
            block_source: self.block_source,
            block_source_utf16: self.block_source_utf16,
            inline_source: self.inline_source,
            inline_source_utf16: self.inline_source_utf16,
            query_receipt: self.query_receipt,
            driver_work: self.driver_work,
            fence,
        })
    }
}

impl M11RecursiveGreenParagraphInlinePreparation {
    pub(crate) fn from_persistent_session(
        block_source: Range<u32>,
        block_source_utf16: Range<u32>,
        inline_source: Range<u32>,
        inline_source_utf16: Range<u32>,
        query_receipt: M11RecursiveGreenQueryReceipt,
        fence: M11RecursiveGreenParagraphFence,
    ) -> Self {
        Self {
            block_source,
            block_source_utf16,
            inline_source,
            inline_source_utf16,
            query_receipt,
            driver_work: 0,
            fence,
        }
    }

    #[must_use]
    pub fn block_source_range(&self) -> Range<u32> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> Range<u32> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub fn inline_source_range(&self) -> Range<u32> {
        self.inline_source.clone()
    }

    #[must_use]
    pub fn inline_source_utf16_range(&self) -> Range<u32> {
        self.inline_source_utf16.clone()
    }

    #[must_use]
    pub const fn query_receipt(&self) -> M11RecursiveGreenQueryReceipt {
        self.query_receipt
    }

    #[must_use]
    pub const fn driver_work(&self) -> usize {
        self.driver_work
    }

    #[must_use]
    pub fn into_fence(self) -> M11RecursiveGreenParagraphFence {
        self.fence
    }
}

/// Builds the capped recursive Green checkpoint, resolves its physical
/// Paragraph owner, and returns an inline job backed only by that fence.
pub fn prepare_m11_recursive_green_inline_leaf(
    runtime: &mut DocumentRuntime,
    point: M11RecursiveGreenPoint,
) -> Result<M11RecursiveGreenInlineLeafPreparation, M11RecursiveGreenParagraphPreparationError> {
    let source = runtime.current_source_version().ok_or(
        M11RecursiveGreenParagraphPreparationError::InvalidState(
            "recursive-Green Paragraph bridge requires an open source",
        ),
    )?;
    if source.byte_len() > M11_RECURSIVE_GREEN_PARAGRAPH_BRIDGE_MAX_SOURCE_BYTES {
        return Err(M11RecursiveGreenParagraphPreparationError::SourceTooLarge {
            observed: source.byte_len(),
            maximum: M11_RECURSIVE_GREEN_PARAGRAPH_BRIDGE_MAX_SOURCE_BYTES,
        });
    }

    let scanner_lease = runtime.snapshot_current_source()?;
    let writer_lease = runtime.snapshot_current_source()?;
    let scanner = SnapshotLineScanner::new(scanner_lease)?;
    let mut controller = M11DirectBlockController::new()?;
    let mut writer = M11BlockWriter::new(runtime, writer_lease)?;
    let mut driver_work = 0;
    let built = drive_document(
        runtime,
        scanner,
        &mut controller,
        &mut writer,
        &mut driver_work,
    );
    let mut root = match built {
        Ok(root) => root,
        Err(error) => {
            cancel_writer(runtime, &mut writer);
            return Err(error);
        }
    };

    let prepared = (|| {
        let limits = M11RecursiveGreenFrameQueryLimits::new(64, 8192, 64, 8192).ok_or(
            M11RecursiveGreenParagraphPreparationError::InvalidState(
                "recursive-Green Paragraph query limits must be nonzero",
            ),
        )?;
        let fence = resolve_m11_recursive_green_inline_leaf_fence(runtime, &root, point, limits)?
            .ok_or(M11RecursiveGreenParagraphPreparationError::NotParagraph)?;
        let block_source = to_u32_range(fence.block_source_range())?;
        let block_source_utf16 = to_u32_range(fence.block_source_utf16_range())?;
        let inline_source = to_u32_range(fence.inline_source_range())?;
        let inline_source_utf16 = to_u32_range(fence.inline_source_utf16_range())?;
        let query_receipt = fence.receipt();
        Ok(M11RecursiveGreenInlineLeafPreparation {
            block_source,
            block_source_utf16,
            inline_source,
            inline_source_utf16,
            query_receipt,
            driver_work,
            fence,
        })
    })();

    release_root(runtime, &mut root)?;
    prepared
}

/// Compatibility wrapper retaining the original Paragraph-only contract.
pub fn prepare_m11_recursive_green_paragraph_inline(
    runtime: &mut DocumentRuntime,
    point: M11RecursiveGreenPoint,
) -> Result<M11RecursiveGreenParagraphInlinePreparation, M11RecursiveGreenParagraphPreparationError>
{
    prepare_m11_recursive_green_inline_leaf(runtime, point)?.into_paragraph()
}

/// Prepares one inline-bearing row from a bounded Green slice without
/// rebuilding or consulting a whole-document structural root.
pub fn prepare_m11_recursive_green_slice_inline_leaf(
    runtime: &DocumentRuntime,
    root: &M11RecursiveGreenSliceRoot,
    point: M11RecursiveGreenPoint,
) -> Result<M11RecursiveGreenInlineLeafPreparation, M11RecursiveGreenParagraphPreparationError> {
    let limits = M11RecursiveGreenRowQueryLimits::new(1, 25, 3_200, 64, 512).ok_or(
        M11RecursiveGreenParagraphPreparationError::InvalidState(
            "recursive-Green slice query limits must be nonzero",
        ),
    )?;
    let fence = resolve_m11_recursive_green_slice_inline_leaf_row_fence(
        runtime, root, point, limits, 8192,
    )?
    .ok_or(M11RecursiveGreenParagraphPreparationError::NotParagraph)?;
    let block_source = to_u32_range(fence.block_source_range())?;
    let block_source_utf16 = to_u32_range(fence.block_source_utf16_range())?;
    let inline_source = to_u32_range(fence.inline_source_range())?;
    let inline_source_utf16 = to_u32_range(fence.inline_source_utf16_range())?;
    let query_receipt = fence.receipt();
    Ok(
        M11RecursiveGreenInlineLeafPreparation::from_persistent_session(
            block_source,
            block_source_utf16,
            inline_source,
            inline_source_utf16,
            query_receipt,
            fence,
        ),
    )
}

/// One bounded-slice renderable-row window with the inline preparation of
/// every qualifying row, all minted by a single shared ancestor-context walk.
///
/// `preparations` is index-aligned with `window.rows()`; `None` marks a row
/// the per-point preparation would reject as `NotParagraph`. Every returned
/// preparation carries the shared walk's query receipt.
pub struct M11RecursiveGreenSliceInlineLeafRowBatch {
    window: M11RecursiveGreenRowWindow,
    preparations: Vec<Option<M11RecursiveGreenInlineLeafPreparation>>,
}

impl M11RecursiveGreenSliceInlineLeafRowBatch {
    #[must_use]
    pub const fn window(&self) -> &M11RecursiveGreenRowWindow {
        &self.window
    }

    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        M11RecursiveGreenRowWindow,
        Vec<Option<M11RecursiveGreenInlineLeafPreparation>>,
    ) {
        (self.window, self.preparations)
    }
}

/// Batch counterpart of [`prepare_m11_recursive_green_slice_inline_leaf`]:
/// locates the bounded row window and prepares every inline-bearing row in
/// one walk instead of re-deriving the shared ancestor context per row. The
/// per-row admission and inline-source cap are identical to the per-point
/// preparation anchored at each row's own start.
pub fn prepare_m11_recursive_green_slice_inline_leaf_rows(
    runtime: &DocumentRuntime,
    root: &M11RecursiveGreenSliceRoot,
    point: M11RecursiveGreenPoint,
    limits: M11RecursiveGreenRowQueryLimits,
) -> Result<M11RecursiveGreenSliceInlineLeafRowBatch, M11RecursiveGreenParagraphPreparationError> {
    let (window, fences) = resolve_m11_recursive_green_slice_inline_leaf_row_fences(
        runtime, root, point, limits, 8192,
    )?;
    let preparations = fences
        .into_iter()
        .map(|fence| {
            fence
                .map(|fence| {
                    let block_source = to_u32_range(fence.block_source_range())?;
                    let block_source_utf16 = to_u32_range(fence.block_source_utf16_range())?;
                    let inline_source = to_u32_range(fence.inline_source_range())?;
                    let inline_source_utf16 = to_u32_range(fence.inline_source_utf16_range())?;
                    let query_receipt = fence.receipt();
                    Ok(
                        M11RecursiveGreenInlineLeafPreparation::from_persistent_session(
                            block_source,
                            block_source_utf16,
                            inline_source,
                            inline_source_utf16,
                            query_receipt,
                            fence,
                        ),
                    )
                })
                .transpose()
        })
        .collect::<Result<Vec<_>, M11RecursiveGreenParagraphPreparationError>>()?;
    Ok(M11RecursiveGreenSliceInlineLeafRowBatch {
        window,
        preparations,
    })
}

fn drive_document(
    runtime: &mut DocumentRuntime,
    mut scanner: SnapshotLineScanner,
    controller: &mut M11DirectBlockController,
    writer: &mut M11BlockWriter,
    work: &mut usize,
) -> Result<M11RecursiveGreenRoot, M11RecursiveGreenParagraphPreparationError> {
    write_pending_command(controller, writer, runtime, work)?;
    loop {
        let line = loop {
            spend(work, 1)?;
            match scanner.poll(SOURCE_CURSOR_WINDOW_BYTES)? {
                SnapshotLinePoll::Pending(next) => scanner = next,
                SnapshotLinePoll::Line(line) => break Some(line),
                SnapshotLinePoll::Complete => break None,
            }
        };
        let Some(line) = line else { break };
        let facts = line.facts();
        let mut source = line.into_source()?;
        spend(work, 1)?;
        let mut admission = <M11DirectBlockController as M11ExactController<
            SnapshotLineSource,
        >>::begin_source_line(controller, facts.identity())?;
        loop {
            if source.access_budget() == 0 && source.position() < source.len() {
                source.replenish_access_budget(SOURCE_CURSOR_WINDOW_BYTES)?;
            }
            let receipt = <M11DirectBlockController as M11ExactController<
                SnapshotLineSource,
            >>::poll_source_line(controller, &mut admission, &mut source, DRIVER_FUEL)?;
            spend(work, receipt.lexical_work_units.max(1))?;
            if receipt.status == M11SourceLinePollStatus::Matched {
                break;
            }
        }
        spend(work, 1)?;
        <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::commit_source_line(
            controller, admission, facts,
        )?;
        scanner = source.finish()?;

        loop {
            let receipt = controller.poll_line(DRIVER_FUEL)?;
            spend(work, receipt.transitions.max(1))?;
            match receipt.status {
                M11DirectBlockPollStatus::Pending => {}
                M11DirectBlockPollStatus::CommandReady => {
                    write_pending_command(controller, writer, runtime, work)?;
                }
                M11DirectBlockPollStatus::ExternalWorkReady => {
                    return Err(M11DirectBlockError::Unsupported(
                        M11DirectBlockUnsupported::ReferenceExternalWork,
                    )
                    .into());
                }
                M11DirectBlockPollStatus::Complete => break,
            }
        }
    }

    spend(work, 1)?;
    controller.begin_finish()?;
    loop {
        let receipt = controller.poll_finish(DRIVER_FUEL)?;
        spend(work, receipt.transitions.max(1))?;
        match receipt.status {
            M11DirectBlockPollStatus::Pending => {}
            M11DirectBlockPollStatus::CommandReady => {
                write_pending_command(controller, writer, runtime, work)?;
            }
            M11DirectBlockPollStatus::ExternalWorkReady => {
                return Err(M11DirectBlockError::Unsupported(
                    M11DirectBlockUnsupported::ReferenceExternalWork,
                )
                .into());
            }
            M11DirectBlockPollStatus::Complete => break,
        }
    }
    writer
        .take_root()
        .ok_or(M11RecursiveGreenParagraphPreparationError::InvalidState(
            "completed direct block writer omitted its recursive Green root",
        ))
}

fn write_pending_command(
    controller: &mut M11DirectBlockController,
    writer: &mut M11BlockWriter,
    runtime: &mut DocumentRuntime,
    work: &mut usize,
) -> Result<(), M11RecursiveGreenParagraphPreparationError> {
    let command = *controller.pending_command().ok_or(
        M11RecursiveGreenParagraphPreparationError::InvalidState(
            "direct block controller omitted its ready command",
        ),
    )?;
    spend(work, 1)?;
    if writer.offer_command(command)? == M11BlockWriterOfferStatus::Pending {
        loop {
            let poll = writer.poll(runtime, DRIVER_FUEL)?;
            spend(work, poll.transitions().max(1))?;
            if matches!(
                poll.status(),
                M11BlockWriterPollStatus::CommandComplete
                    | M11BlockWriterPollStatus::DocumentComplete
            ) {
                break;
            }
        }
    }
    spend(work, 1)?;
    controller.acknowledge_command()?;
    Ok(())
}

fn spend(
    work: &mut usize,
    amount: usize,
) -> Result<(), M11RecursiveGreenParagraphPreparationError> {
    *work = work
        .checked_add(amount)
        .ok_or(M11RecursiveGreenParagraphPreparationError::WorkBoundExceeded)?;
    if *work > MAXIMUM_DRIVER_WORK {
        return Err(M11RecursiveGreenParagraphPreparationError::WorkBoundExceeded);
    }
    Ok(())
}

fn to_u32_range(
    range: Range<u64>,
) -> Result<Range<u32>, M11RecursiveGreenParagraphPreparationError> {
    Ok(u32::try_from(range.start)
        .map_err(|_| M11RecursiveGreenParagraphPreparationError::WorkBoundExceeded)?
        ..u32::try_from(range.end)
            .map_err(|_| M11RecursiveGreenParagraphPreparationError::WorkBoundExceeded)?)
}

fn release_root(
    runtime: &mut DocumentRuntime,
    root: &mut M11RecursiveGreenRoot,
) -> Result<(), M11RecursiveGreenParagraphPreparationError> {
    root.begin_release(runtime)
        .map_err(M11BlockWriterError::from)?;
    loop {
        let poll = root
            .poll_release(runtime, RELEASE_FUEL)
            .map_err(M11BlockWriterError::from)?;
        if poll.complete() {
            return Ok(());
        }
    }
}

fn cancel_writer(runtime: &mut DocumentRuntime, writer: &mut M11BlockWriter) {
    if writer.begin_cancel(runtime).is_err() {
        return;
    }
    loop {
        match writer.poll_cancel(runtime, RELEASE_FUEL) {
            Ok(poll) if poll.complete() => return,
            Ok(_) => {}
            Err(_) => return,
        }
    }
}
