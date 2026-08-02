// SPDX-License-Identifier: MIT

//! Parser-semantic inline-leaf selection over generic recursive Green storage.

use std::{fmt, ops::Range};

use flark_engine::parser_internal::{
    M11ParserSourceRangeAuthority, M11RecursiveGreenFrameFence, M11RecursiveGreenFrameId,
    M11RecursiveGreenFrameQueryError, M11RecursiveGreenFrameQueryLimits, M11RecursiveGreenKind,
    M11RecursiveGreenPoint, M11RecursiveGreenQueryReceipt, M11RecursiveGreenRoot,
    M11RecursiveGreenRowQueryLimits,
};
use flark_engine::{DocumentRuntime, SourceVersion};

use super::writer::{KIND_HEADING, KIND_PARAGRAPH};

/// Parser-owned inline-bearing recursive-Green leaf kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenInlineLeafKind {
    Paragraph,
    Heading,
}

impl M11RecursiveGreenInlineLeafKind {
    #[must_use]
    pub const fn from_green_kind(kind: M11RecursiveGreenKind) -> Option<Self> {
        match kind.get() {
            KIND_PARAGRAPH => Some(Self::Paragraph),
            KIND_HEADING => Some(Self::Heading),
            _ => None,
        }
    }

    const fn green_kind(self) -> M11RecursiveGreenKind {
        let value = match self {
            Self::Paragraph => KIND_PARAGRAPH,
            Self::Heading => KIND_HEADING,
        };
        match M11RecursiveGreenKind::new(value) {
            Some(kind) => kind,
            None => unreachable!(),
        }
    }
}

/// Move-only proof that a point belongs to one final inline-bearing leaf.
#[must_use = "inline-leaf fences must be consumed by exact inline work or deliberately dropped"]
pub struct M11RecursiveGreenInlineLeafFence {
    inner: M11RecursiveGreenFrameFence,
    kind: M11RecursiveGreenInlineLeafKind,
}

impl fmt::Debug for M11RecursiveGreenInlineLeafFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11RecursiveGreenInlineLeafFence")
            .field("kind", &self.kind)
            .field("source", &self.source())
            .field("frame", &self.frame())
            .field("block_source", &self.block_source_range())
            .field("inline_source", &self.inline_source_range())
            .field("receipt", &self.receipt())
            .finish_non_exhaustive()
    }
}

impl M11RecursiveGreenInlineLeafFence {
    #[must_use]
    pub const fn kind(&self) -> M11RecursiveGreenInlineLeafKind {
        self.kind
    }

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
    pub fn inline_source_range(&self) -> Range<u64> {
        self.inner.inline_source_range()
    }

    #[must_use]
    pub fn inline_source_utf16_range(&self) -> Range<u64> {
        self.inner.inline_source_utf16_range()
    }

    #[must_use]
    pub const fn receipt(&self) -> M11RecursiveGreenQueryReceipt {
        self.inner.receipt()
    }

    pub(crate) fn into_inline_authority(self) -> (M11ParserSourceRangeAuthority, Range<u64>) {
        self.inner.into_inline_authority()
    }

    pub(crate) fn into_paragraph(self) -> Option<M11RecursiveGreenParagraphFence> {
        (self.kind == M11RecursiveGreenInlineLeafKind::Paragraph)
            .then_some(M11RecursiveGreenParagraphFence { inner: self })
    }
}

/// Move-only proof that a point belongs to one final Paragraph frame.
///
/// The generic storage query mints every range and the exact source authority;
/// this parser wrapper contributes only the grammar-owned Paragraph kind.
#[must_use = "Paragraph fences must be consumed by exact inline work or deliberately dropped"]
pub struct M11RecursiveGreenParagraphFence {
    inner: M11RecursiveGreenInlineLeafFence,
}

impl fmt::Debug for M11RecursiveGreenParagraphFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11RecursiveGreenParagraphFence")
            .field("source", &self.source())
            .field("frame", &self.frame())
            .field("block_source", &self.block_source_range())
            .field("block_source_utf16", &self.block_source_utf16_range())
            .field("inline_source", &self.inline_source_range())
            .field("inline_source_utf16", &self.inline_source_utf16_range())
            .field("receipt", &self.receipt())
            .finish_non_exhaustive()
    }
}

impl M11RecursiveGreenParagraphFence {
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
    pub fn inline_source_range(&self) -> Range<u64> {
        self.inner.inline_source_range()
    }

    #[must_use]
    pub fn inline_source_utf16_range(&self) -> Range<u64> {
        self.inner.inline_source_utf16_range()
    }

    #[must_use]
    pub const fn receipt(&self) -> M11RecursiveGreenQueryReceipt {
        self.inner.receipt()
    }

    pub(crate) fn into_inline_authority(self) -> (M11ParserSourceRangeAuthority, Range<u64>) {
        self.inner.into_inline_authority()
    }

    pub(crate) fn into_inline_leaf(self) -> M11RecursiveGreenInlineLeafFence {
        self.inner
    }
}

/// Resolves an exact final Paragraph or Heading owner and mints its contiguous
/// parser-authored inline range. No source range is caller supplied.
pub fn resolve_m11_recursive_green_inline_leaf_fence(
    runtime: &DocumentRuntime,
    root: &M11RecursiveGreenRoot,
    point: M11RecursiveGreenPoint,
    limits: M11RecursiveGreenFrameQueryLimits,
) -> Result<Option<M11RecursiveGreenInlineLeafFence>, M11RecursiveGreenFrameQueryError> {
    let expected = [
        M11RecursiveGreenInlineLeafKind::Paragraph.green_kind(),
        M11RecursiveGreenInlineLeafKind::Heading.green_kind(),
    ];
    root.locate_frame_fence_for_kinds(runtime, point, &expected, limits)
        .map(|fence| {
            fence.map(|inner| M11RecursiveGreenInlineLeafFence {
                kind: M11RecursiveGreenInlineLeafKind::from_green_kind(inner.kind())
                    .expect("accepted Green kind is inline-bearing"),
                inner,
            })
        })
}

/// Resolves an inline-bearing row through cached parser-authored close
/// geometry, avoiding replay from a potentially distant frame `Enter`.
pub fn resolve_m11_recursive_green_inline_leaf_row_fence(
    runtime: &DocumentRuntime,
    root: &M11RecursiveGreenRoot,
    point: M11RecursiveGreenPoint,
    limits: M11RecursiveGreenRowQueryLimits,
    maximum_inline_source_bytes: u64,
) -> Result<Option<M11RecursiveGreenInlineLeafFence>, M11RecursiveGreenFrameQueryError> {
    let expected = [
        M11RecursiveGreenInlineLeafKind::Paragraph.green_kind(),
        M11RecursiveGreenInlineLeafKind::Heading.green_kind(),
    ];
    root.locate_renderable_row_fence_for_kinds(
        runtime,
        point,
        &expected,
        limits,
        maximum_inline_source_bytes,
    )
    .map(|fence| {
        fence.map(|inner| M11RecursiveGreenInlineLeafFence {
            kind: M11RecursiveGreenInlineLeafKind::from_green_kind(inner.kind())
                .expect("accepted Green kind is inline-bearing"),
            inner,
        })
    })
}

/// Resolves the physical coverage owner at one authenticated source point and
/// returns it only when its final kind is Paragraph. The caller cannot provide
/// or widen either returned range.
pub fn resolve_m11_recursive_green_paragraph_fence(
    runtime: &DocumentRuntime,
    root: &M11RecursiveGreenRoot,
    point: M11RecursiveGreenPoint,
    limits: M11RecursiveGreenFrameQueryLimits,
) -> Result<Option<M11RecursiveGreenParagraphFence>, M11RecursiveGreenFrameQueryError> {
    resolve_m11_recursive_green_inline_leaf_fence(runtime, root, point, limits).map(|fence| {
        fence.and_then(|inner| {
            (inner.kind() == M11RecursiveGreenInlineLeafKind::Paragraph)
                .then_some(M11RecursiveGreenParagraphFence { inner })
        })
    })
}
