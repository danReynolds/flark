//! Infrastructure-only composition slice for Flark's v3 live Markdown runtime.
//!
//! This crate deliberately contains no Markdown grammar, prediction layer, or
//! dependency on the superseded `integrated_parser_slice` parser. It proves
//! source revision lineage, persistent-page lifetime, and latest-wins root
//! publication before the exact block core is attached.

// This is an executable feasibility slice, not a stabilized public package API.
// Keep the surface compact while Clippy still checks every substantive lint.
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

pub mod arena;
pub mod candidate_writer;
pub mod checkpoint_authority;
#[allow(dead_code)] // Storage-only proof; deliberately not restart authority yet.
pub(crate) mod committed_checkpoint_index;
pub mod coordinator;
pub mod event_tape;
#[cfg(feature = "exact-parser")]
#[allow(dead_code)] // Executed by the selection-gate tests before package wiring.
mod exact_block_job;
pub mod generic_green;
pub mod green_tree;
pub mod hierarchical_green_sequence;
#[cfg(feature = "host-mirror-probe")]
#[allow(dead_code)] // Host-owned page-splice feasibility gate; publication carry follows.
pub(crate) mod host_mirror;
#[cfg(feature = "host-publication-staging-probe")]
#[allow(dead_code)] // Standalone staged-publication mechanism gate; host wiring follows.
pub(crate) mod host_publication_staging;
pub mod identity;
#[cfg(feature = "exact-parser")]
#[allow(dead_code)] // Donor-only recipe; product restart authority is intentionally absent.
mod indexed_donor_checkpoint;
#[cfg(feature = "inline-liveness-probe")]
pub mod inline_leaf_liveness_probe;
#[cfg(feature = "exact-parser")]
pub mod inline_leaf_presentation;
pub mod live_document;
#[cfg(feature = "exact-parser")]
#[allow(dead_code)] // Integrated convergence gate; publication wiring follows.
pub(crate) mod parent_selected_convergence;
#[allow(dead_code)] // Shared immutable-value kernel; terminal reference wiring follows.
pub(crate) mod persistent_blob;
#[cfg(feature = "exact-parser")]
mod reference_value_blob;
mod persistent_sequence;
pub mod presentation;
pub mod projection_reset;
pub mod record_forest;
#[allow(dead_code)] // Production-shaped exact label interner; terminal join wiring follows.
pub(crate) mod reference_label_interner;
#[allow(dead_code)] // Private contiguous-restart/re-winner feasibility gate.
pub(crate) mod reference_restart_index;
#[allow(dead_code)] // Private writer-owned reference-index feasibility gate.
pub(crate) mod reference_semantic_index;
#[allow(dead_code)] // Source-only cross-build authority proof; actor composition is the next gate.
pub(crate) mod retained_restart_coordinate;
#[cfg(feature = "exact-parser")]
#[allow(dead_code)] // Executed by the same-build checkpoint selection gate.
mod same_build_checkpoint;
pub mod serialized_green;
#[cfg(feature = "exact-parser")]
#[allow(dead_code)] // Narrow in-memory mechanism proof; not durable publication.
mod setext_cross_build_restart;
pub mod source;
pub mod source_bound_ledger;
pub mod source_projection_composer;
#[allow(dead_code)] // Storage-only ownership proof; deliberately not restart authority yet.
pub(crate) mod storage_only_composite_document;
#[cfg(feature = "exact-parser")]
#[allow(dead_code)] // Authenticated Table cursor/join mechanism; builder minting is pending.
mod table_projection_cursor_gate;

pub use arena::*;
pub use candidate_writer::*;
pub use checkpoint_authority::*;
pub use coordinator::*;
pub use event_tape::*;
pub use generic_green::*;
pub use green_tree::*;
pub use hierarchical_green_sequence::*;
pub use identity::*;
#[cfg(feature = "inline-liveness-probe")]
pub use inline_leaf_liveness_probe::*;
#[cfg(feature = "exact-parser")]
pub use inline_leaf_presentation::*;
pub use live_document::*;
pub use presentation::*;
pub use projection_reset::*;
pub use record_forest::*;
pub use serialized_green::*;
pub use source::*;
pub use source_bound_ledger::*;
pub use source_projection_composer::*;

/// Prevents this infrastructure gate from being mistaken for a parser.
pub const MARKDOWN_GRAMMAR_ATTACHED: bool = false;
