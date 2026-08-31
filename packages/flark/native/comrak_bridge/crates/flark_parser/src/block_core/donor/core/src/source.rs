use std::ops::Range;

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
    pub const fn new(start: u32, end: u32) -> Self {
        Self {
            start,
            end,
            append_newline: false,
        }
    }

    #[must_use]
    pub const fn with_newline(mut self) -> Self {
        self.append_newline = true;
        self
    }

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

impl LeafContent {
    #[must_use]
    pub fn logical_len(&self) -> usize {
        self.source_backed.map_or(self.logical.len(), |metadata| {
            usize::try_from(metadata.logical_len).expect("logical length fits usize")
        })
    }

    /// Select raw source-backed storage even when the block contributes zero
    /// logical bytes (for example, a bare opening fence at end of input).
    pub fn ensure_source_backed(&mut self) {
        assert!(self.logical.is_empty());
        self.source_backed.get_or_insert_default();
    }

    pub fn push_source(
        &mut self,
        leaf_id: u64,
        source: Range<usize>,
        text: &str,
        transform: OriginTransform,
    ) {
        assert!(self.source_backed.is_none());
        let logical_start = self.logical.len();
        self.logical.push_str(text);
        self.origins.push(OriginRun {
            logical_start: logical_start.try_into().expect("logical leaf below u32"),
            logical_len: text.len().try_into().expect("logical leaf below u32"),
            source: Some(CoverageRange {
                leaf_id,
                start: source.start.try_into().expect("coverage leaf below u32"),
                end: source.end.try_into().expect("coverage leaf below u32"),
            }),
            transform,
        });
    }

    /// Start one physical line in a raw source-backed code/HTML leaf.
    pub fn start_source_backed_line(&mut self, source_offset: usize) -> u32 {
        assert!(self.logical.is_empty());
        self.ensure_source_backed();
        let logical_len = self
            .source_backed
            .as_ref()
            .expect("source-backed storage was selected")
            .logical_len;
        self.line_offsets.push(source_offset);
        logical_len
    }

    /// Append one coverage-relative run without copying its logical bytes.
    pub fn push_source_backed(
        &mut self,
        leaf_id: u64,
        source: Range<usize>,
        logical_len: usize,
        transform: OriginTransform,
    ) {
        assert!(self.logical.is_empty());
        let metadata = self
            .source_backed
            .as_mut()
            .expect("source-backed line was started");
        let logical_len = u32::try_from(logical_len).expect("logical run below u32");
        self.origins.push(OriginRun {
            logical_start: metadata.logical_len,
            logical_len,
            source: Some(CoverageRange {
                leaf_id,
                start: source.start.try_into().expect("coverage leaf below u32"),
                end: source.end.try_into().expect("coverage leaf below u32"),
            }),
            transform,
        });
        metadata.logical_len = metadata
            .logical_len
            .checked_add(logical_len)
            .expect("logical leaf below u32");
    }

    /// Fold one completed physical line into constant-size finalization data.
    pub fn finish_source_backed_line(&mut self, line_start: u32, identity_text: &str) {
        let metadata = self
            .source_backed
            .as_mut()
            .expect("source-backed line was started");
        let line_end = metadata.logical_len;
        let newline_bytes = identity_text
            .bytes()
            .rev()
            .take_while(|byte| matches!(byte, b'\r' | b'\n'))
            .count();
        let content_end = line_end
            .checked_sub(u32::try_from(newline_bytes).expect("line ending below u32"))
            .expect("line ending belongs to current line");
        if metadata.lines == 0 {
            metadata.first_line_content_end = content_end;
            metadata.first_line_end = line_end;
        }
        if identity_text
            .bytes()
            .any(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        {
            metadata.trimmed_end = content_end;
            metadata.trimmed_line_index = metadata.lines;
            metadata.trimmed_last_line_len = content_end
                .checked_sub(line_start)
                .expect("line content follows its start");
        }
        metadata.lines = metadata.lines.checked_add(1).expect("line count below u32");
    }
}
