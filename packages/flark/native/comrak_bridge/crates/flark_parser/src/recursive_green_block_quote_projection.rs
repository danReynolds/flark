//! Parser-semantic authority for one top-level recursive-Green block quote.
//!
//! The engine authenticates the physical frame, exact single-Paragraph shape,
//! and projected metrics. The parser wrapper contributes only Markdown kind
//! identities before the independent marked-line projection job consumes the
//! move-only source authority.

use std::{fmt, ops::Range};

use flark_engine::parser_internal::{
    M11ParserSourceRangeAuthority, M11RecursiveGreenFrameId, M11RecursiveGreenFrameQueryError,
    M11RecursiveGreenPoint, M11RecursiveGreenProjectedFrameFence, M11RecursiveGreenQueryReceipt,
    M11RecursiveGreenRoot, M11RecursiveGreenRowQueryLimits,
};
use flark_engine::{DocumentRuntime, SourceVersion};

use crate::block_core::{KIND_BLOCK_QUOTE, KIND_DOCUMENT, KIND_PARAGRAPH};

/// Move-only proof of one exact top-level BlockQuote containing one Paragraph.
#[must_use = "block-quote projection fences must be consumed by exact projection work or deliberately dropped"]
pub struct M11RecursiveGreenBlockQuoteProjectionFence {
    inner: M11RecursiveGreenProjectedFrameFence,
}

impl fmt::Debug for M11RecursiveGreenBlockQuoteProjectionFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11RecursiveGreenBlockQuoteProjectionFence")
            .field("source", &self.source())
            .field("frame", &self.frame())
            .field("block_source", &self.block_source_range())
            .field("block_source_utf16", &self.block_source_utf16_range())
            .field("line_count", &self.line_count())
            .field("projected_utf8_length", &self.projected_utf8_length())
            .field("projected_utf16_length", &self.projected_utf16_length())
            .field("receipt", &self.receipt())
            .finish_non_exhaustive()
    }
}

impl M11RecursiveGreenBlockQuoteProjectionFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.inner.source()
    }

    #[must_use]
    pub const fn frame(&self) -> M11RecursiveGreenFrameId {
        self.inner.frame()
    }

    #[must_use]
    pub fn block_source_range(&self) -> Range<u64> {
        self.inner.block_source_range()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> Range<u64> {
        self.inner.block_source_utf16_range()
    }

    #[must_use]
    pub const fn line_count(&self) -> u64 {
        self.inner.line_count()
    }

    #[must_use]
    pub const fn projected_utf8_length(&self) -> u64 {
        self.inner.projected_source_metric().bytes()
    }

    #[must_use]
    pub const fn projected_utf16_length(&self) -> u64 {
        self.inner.projected_source_metric().utf16()
    }

    #[must_use]
    pub const fn receipt(&self) -> M11RecursiveGreenQueryReceipt {
        self.inner.receipt()
    }

    pub(crate) fn into_source_authority(self) -> (M11ParserSourceRangeAuthority, Range<u64>) {
        self.inner.into_projected_authority()
    }
}

/// Wire-sized preparation retained until endpoint staging starts projection.
pub struct M11RecursiveGreenBlockQuoteProjectionPreparation {
    block_source: Range<u32>,
    block_source_utf16: Range<u32>,
    query_receipt: M11RecursiveGreenQueryReceipt,
    fence: M11RecursiveGreenBlockQuoteProjectionFence,
}

impl M11RecursiveGreenBlockQuoteProjectionPreparation {
    pub(crate) fn from_persistent_session(
        block_source: Range<u32>,
        block_source_utf16: Range<u32>,
        query_receipt: M11RecursiveGreenQueryReceipt,
        fence: M11RecursiveGreenBlockQuoteProjectionFence,
    ) -> Self {
        Self {
            block_source,
            block_source_utf16,
            query_receipt,
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
    pub const fn frame(&self) -> M11RecursiveGreenFrameId {
        self.fence.frame()
    }

    #[must_use]
    pub const fn query_receipt(&self) -> M11RecursiveGreenQueryReceipt {
        self.query_receipt
    }

    #[must_use]
    pub fn into_fence(self) -> M11RecursiveGreenBlockQuoteProjectionFence {
        self.fence
    }
}

/// Resolves the exact top-level `Document -> BlockQuote -> Paragraph` shape.
/// The complete container is capped and independently folded from Green.
pub fn resolve_m11_recursive_green_block_quote_projection_fence(
    runtime: &DocumentRuntime,
    root: &M11RecursiveGreenRoot,
    point: M11RecursiveGreenPoint,
    limits: M11RecursiveGreenRowQueryLimits,
    maximum_source_bytes: u64,
) -> Result<Option<M11RecursiveGreenBlockQuoteProjectionFence>, M11RecursiveGreenFrameQueryError> {
    let document = flark_engine::parser_internal::M11RecursiveGreenKind::new(KIND_DOCUMENT)
        .expect("Document Green kind is nonzero");
    let block_quote = flark_engine::parser_internal::M11RecursiveGreenKind::new(KIND_BLOCK_QUOTE)
        .expect("BlockQuote Green kind is nonzero");
    let paragraph = flark_engine::parser_internal::M11RecursiveGreenKind::new(KIND_PARAGRAPH)
        .expect("Paragraph Green kind is nonzero");
    root.locate_single_child_projected_container_fence(
        runtime,
        point,
        document,
        block_quote,
        paragraph,
        limits,
        maximum_source_bytes,
    )
    .map(|fence| fence.map(|inner| M11RecursiveGreenBlockQuoteProjectionFence { inner }))
}
