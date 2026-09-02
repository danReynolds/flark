//! Persistent packed recursive-green storage.
//!
//! Markdown grammar remains in `flark-parser`. This module owns only the
//! generic event algebra, exact source/logical geometry, persistent measured
//! storage, and source-bound build/query lifecycle.

mod adopt;
mod build;
mod codec;
mod fragment;
mod query;
mod semantic;
mod splice;

#[cfg(test)]
mod tests;

pub use adopt::*;
pub use build::*;
pub use codec::*;
pub use fragment::*;
pub use query::*;
pub use semantic::*;
