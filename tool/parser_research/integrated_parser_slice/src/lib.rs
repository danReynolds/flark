//! Integrated commitment slice for RFC 023.
//!
//! Unlike the earlier isolated spikes, this crate is allowed to count as
//! evidence only when source editing, block-to-inline mapping, real inline
//! algorithms, packed state/facts, exact restart, references, and persistent
//! deltas use one runtime representation and one fuelled job.

pub mod arena;
pub mod block;
pub mod convergence;
#[cfg(feature = "crop-research")]
pub mod crop_source;
pub mod execution;
pub mod frontier;
pub mod grammar;
pub mod inline_machine;
pub mod lifetime;
pub mod owned_parse;
pub mod packed;
pub mod scheduler;
pub mod source;

/// This crate begins as a falsifiable vertical slice, not a production parser.
/// Gaps are removed only when their replacement is executable and measured.
pub const OPEN_GAPS: &[&str] = &[
    "the owned block-to-inline composition still implements only the declared narrow Markdown subset",
    "block and shared-lexer allocation work lacks complete next-operation scheduler preflight",
    "temporary block/lexer Arc graphs are not yet reclaimed through the bounded physical arena",
    "the physical output is still a reverse construction chain rather than the final visible-page index",
    "checkpoint restart and exact suffix convergence are not integrated yet",
    "reference symbols and dependency generations are not integrated yet",
    "worker transport and physical-device frame scheduling are not integrated yet",
    "the selected Gate A and Gate B composition matrix does not pass yet",
];

/// Prevents the research crate from being mistaken for the package parser.
///
/// # Errors
///
/// Always returns the current explicit research gaps until the integrated
/// acceptance and resource gates have actually passed.
pub fn require_production_ready() -> Result<(), &'static [&'static str]> {
    Err(OPEN_GAPS)
}
