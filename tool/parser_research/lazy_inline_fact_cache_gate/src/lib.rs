//! Lazy exact-inline cache gate.
//!
//! The block/source spine remains complete and source-visible. Exact Comrak
//! inline facts are parsed only for the visible, overscan, and actively edited
//! leaves, then retained in a strict byte-bounded cache.

mod cache;
mod document;
mod reference;

pub use cache::*;
pub use document::*;
pub use reference::*;
