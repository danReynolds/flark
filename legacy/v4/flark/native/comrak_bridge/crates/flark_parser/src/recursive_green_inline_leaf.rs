//! Bounded recursive-Green inline-leaf preparation.
//!
//! Every source range and move-only capability comes from parser-authored row
//! geometry; callers cannot provide or widen a range.

use std::{fmt, ops::Range};

use flark_engine::parser_internal::{
    M11RecursiveGreenFrameQueryError, M11RecursiveGreenPoint, M11RecursiveGreenQueryReceipt,
    M11RecursiveGreenRowQueryLimits, M11RecursiveGreenRowWindow, M11RecursiveGreenSliceRoot,
};
use flark_engine::DocumentRuntime;

use crate::block_core::{
    resolve_m11_recursive_green_slice_inline_leaf_row_fence,
    resolve_m11_recursive_green_slice_inline_leaf_row_fences, M11RecursiveGreenInlineLeafFence,
    M11RecursiveGreenInlineLeafKind,
};

#[derive(Debug)]
pub enum M11RecursiveGreenInlineLeafPreparationError {
    RangeOutOfBounds,
    NotInlineLeaf,
    Query(M11RecursiveGreenFrameQueryError),
    InvalidState(&'static str),
}

impl fmt::Display for M11RecursiveGreenInlineLeafPreparationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RangeOutOfBounds => {
                formatter.write_str("recursive-Green inline-leaf range exceeds u32")
            }
            Self::NotInlineLeaf => {
                formatter.write_str("recursive-Green point is not owned by a final inline leaf")
            }
            Self::Query(error) => error.fmt(formatter),
            Self::InvalidState(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for M11RecursiveGreenInlineLeafPreparationError {}

impl From<M11RecursiveGreenFrameQueryError> for M11RecursiveGreenInlineLeafPreparationError {
    fn from(error: M11RecursiveGreenFrameQueryError) -> Self {
        Self::Query(error)
    }
}

/// Exact recursive-Green Paragraph-or-Heading result ready for atomic inline
/// capture.
pub struct M11RecursiveGreenInlineLeafPreparation {
    block_source: Range<u32>,
    block_source_utf16: Range<u32>,
    inline_source: Range<u32>,
    inline_source_utf16: Range<u32>,
    query_receipt: M11RecursiveGreenQueryReceipt,
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
    pub fn into_fence(self) -> M11RecursiveGreenInlineLeafFence {
        self.fence
    }
}

/// Prepares one inline-bearing row from a bounded Green slice without
/// rebuilding or consulting a whole-document structural root.
pub fn prepare_m11_recursive_green_slice_inline_leaf(
    runtime: &DocumentRuntime,
    root: &M11RecursiveGreenSliceRoot,
    point: M11RecursiveGreenPoint,
) -> Result<M11RecursiveGreenInlineLeafPreparation, M11RecursiveGreenInlineLeafPreparationError> {
    let limits = M11RecursiveGreenRowQueryLimits::new(1, 25, 3_200, 64, 512).ok_or(
        M11RecursiveGreenInlineLeafPreparationError::InvalidState(
            "recursive-Green slice query limits must be nonzero",
        ),
    )?;
    let fence = resolve_m11_recursive_green_slice_inline_leaf_row_fence(
        runtime, root, point, limits, 8192,
    )?
    .ok_or(M11RecursiveGreenInlineLeafPreparationError::NotInlineLeaf)?;
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
/// the per-point preparation would reject as `NotInlineLeaf`. Every returned
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
) -> Result<M11RecursiveGreenSliceInlineLeafRowBatch, M11RecursiveGreenInlineLeafPreparationError> {
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
        .collect::<Result<Vec<_>, M11RecursiveGreenInlineLeafPreparationError>>()?;
    Ok(M11RecursiveGreenSliceInlineLeafRowBatch {
        window,
        preparations,
    })
}

fn to_u32_range(
    range: Range<u64>,
) -> Result<Range<u32>, M11RecursiveGreenInlineLeafPreparationError> {
    Ok(u32::try_from(range.start)
        .map_err(|_| M11RecursiveGreenInlineLeafPreparationError::RangeOutOfBounds)?
        ..u32::try_from(range.end)
            .map_err(|_| M11RecursiveGreenInlineLeafPreparationError::RangeOutOfBounds)?)
}
