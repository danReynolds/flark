use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageRange {
    pub leaf_id: u64,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OriginTransform {
    Identity,
    TabExpansion,
    TrimAndUnescapePipes,
    EntityAndBackslashNormalization,
    Synthetic,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginRun {
    pub logical_start: u32,
    pub logical_len: u32,
    pub source: Option<CoverageRange>,
    pub transform: OriginTransform,
}

/// A bounded view over logical block content. The view never owns the payload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalProjection {
    pub start: u32,
    pub end: u32,
    pub append_newline: bool,
}

impl LogicalProjection {
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end && !self.append_newline
    }
}

/// Constant-size folds maintained while a raw code/HTML block is open.
///
/// The source-backed run vector remains in [`LeafContent::origins`]. These
/// fields let finalization derive info/literal views and HTML source positions
/// without scanning or copying the aggregate payload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceBackedContent {
    pub logical_len: u32,
    pub first_line_content_end: u32,
    pub first_line_end: u32,
    pub trimmed_end: u32,
    pub trimmed_line_index: u32,
    pub trimmed_last_line_len: u32,
    pub lines: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeafContent {
    pub logical: String,
    pub origins: Vec<OriginRun>,
    pub line_offsets: Vec<usize>,
    /// Present for code/HTML leaves whose bytes stay in immutable coverage
    /// leaves. Such leaves keep `logical` empty and retain only scalar runs.
    pub source_backed: Option<SourceBackedContent>,
}
