//! Spec-first Flark-owned Markdown parser trial.
//!
//! This crate intentionally has no production Markdown-parser dependency. It
//! emits editor-oriented source facts; [`render_html`] exists only to certify
//! the semantic tree against the CommonMark examples.

mod block_machine;
mod incremental;
mod model;
mod parser;
mod render;
mod source;
mod unified_slice;

pub use block_machine::{BlockState, LeafState};
pub use incremental::{
    AdvanceStatus, CancelFlag, EditRequest, IncrementalDelta, IncrementalError, ReferenceDelta,
    RevisionedDocument, WorkBudget,
};
pub use model::{
    Block, BlockKind, CoverageLeaf, Document, Inline, InlineKind, Marker, MarkerKind, SourceRange,
};
pub use parser::parse;
pub use render::render_html;
pub use source::SourceRope;
pub use unified_slice::{
    ChunkView, ContainerShape, LeafShape, TableAlignment, UnifiedDelta, UnifiedSliceDocument,
};
