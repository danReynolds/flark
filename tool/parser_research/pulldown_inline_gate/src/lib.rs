//! Bounded inline-donor feasibility spike for Flark.
//!
//! This is an experiment, not a production parser. It tests whether the
//! hardest selected Pulldown inline algorithms can run over segmented input,
//! suspend during both scanning and resolution, and emit exact direct facts
//! without retaining Pulldown's mutable `Tree<Item>`.

mod engine;
mod input;
mod model;

pub use engine::InlineEngine;
pub use input::{InputError, LogicalLeaf, Segment};
pub use model::{
    normalize_reference_label, CancellationToken, Fact, FactKind, MemoryReceipt, ParsePoll,
    ReferenceTable,
};

use std::sync::Arc;

/// Convenience clean parse used by the differential tests. Production callers
/// should hold the engine and schedule `resume` with their own work quantum.
///
/// # Panics
///
/// Panics if `fuel` is zero or the leaf exceeds the packed-offset limit.
#[must_use]
pub fn parse_to_completion(
    leaf: LogicalLeaf,
    references: Arc<ReferenceTable>,
    fuel: usize,
) -> InlineEngine {
    assert!(fuel > 0, "a scheduler quantum must make progress");
    let cancellation = CancellationToken::default();
    let mut engine = InlineEngine::new(leaf, references);
    loop {
        match engine.resume(fuel, &cancellation) {
            ParsePoll::Ready { .. } => return engine,
            ParsePoll::Pending { .. } => {}
            ParsePoll::Cancelled { .. } => unreachable!("token was not cancelled"),
        }
    }
}
