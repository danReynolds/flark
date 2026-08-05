//! Production-shaped feasibility slice for a Flark-owned, Pulldown-derived
//! incremental Markdown core.
//!
//! This is deliberately a narrow parser rather than a wrapper around
//! `pulldown-cmark::Parser`. The block scanners and transition ordering are
//! derived from Pulldown-Cmark 0.13.4, while state, output, checkpoints, fuel,
//! and edit splicing are Flark-owned value types.

mod engine;
mod model;
mod scanners;

pub use engine::{Document, ParserTask};
pub use model::{
    AdvanceReceipt, Ancestry, Chunk, ChunkKind, Container, EditReceipt, Fuel, HeadingLevel,
    MarkerFact, MarkerKind, MemoryReceipt, OutputDelta, ParseError, ProductionGap, SemanticChunk,
    SemanticFact, SemanticSnapshot, Span, PRODUCTION_GAPS,
};

/// This spike must not be mistaken for a production-ready parser.
///
/// Keeping this as an executable API makes architectural omissions visible to
/// callers and tests instead of leaving them as prose-only caveats.
pub fn require_production_ready() -> Result<(), &'static [ProductionGap]> {
    Err(PRODUCTION_GAPS)
}
