use std::collections::HashMap;
use std::ops::Range;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageLeaf {
    pub id: u64,
    /// Revision-local query aid. Persistent facts never retain this value.
    pub absolute_start: usize,
    pub text: String,
}

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
    pub const fn len(self) -> u32 {
        self.end - self.start + if self.append_newline { 1 } else { 0 }
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

    #[must_use]
    pub const fn is_source_backed(&self) -> bool {
        self.source_backed.is_some()
    }

    /// Select raw source-backed storage even when the block contributes zero
    /// logical bytes (for example, a bare opening fence at end of input).
    pub fn ensure_source_backed(&mut self) {
        assert!(self.logical.is_empty());
        self.source_backed.get_or_insert_default();
    }

    pub fn is_blank(&self) -> bool {
        debug_assert!(self.source_backed.is_none());
        self.logical.bytes().all(|byte| byte.is_ascii_whitespace())
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

    pub fn push_synthetic(&mut self, text: &str, transform: OriginTransform) {
        assert!(self.source_backed.is_none());
        let logical_start = self.logical.len();
        self.logical.push_str(text);
        self.origins.push(OriginRun {
            logical_start: logical_start.try_into().expect("logical leaf below u32"),
            logical_len: text.len().try_into().expect("logical leaf below u32"),
            source: None,
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

    /// Remove a donor-recognized leading logical prefix while keeping every
    /// surviving fact coverage-leaf-relative.
    pub fn drain_prefix(&mut self, bytes: usize) {
        assert!(self.source_backed.is_none());
        assert!(bytes <= self.logical.len() && self.logical.is_char_boundary(bytes));
        self.logical.drain(..bytes);
        let mut next = Vec::with_capacity(self.origins.len());
        for mut run in self.origins.drain(..) {
            let start = run.logical_start as usize;
            let end = start + run.logical_len as usize;
            if end <= bytes {
                continue;
            }
            if start < bytes {
                let removed = bytes - start;
                run.logical_len -= u32::try_from(removed).expect("removed below u32");
                if run.transform == OriginTransform::Identity
                    && let Some(source) = &mut run.source
                {
                    source.start += u32::try_from(removed).expect("removed below u32");
                }
                run.logical_start = 0;
            } else {
                run.logical_start -= u32::try_from(bytes).expect("prefix below u32");
            }
            next.push(run);
        }
        self.origins = next;
        self.line_offsets.retain(|offset| *offset >= bytes);
        for offset in &mut self.line_offsets {
            *offset -= bytes;
        }
    }

    pub fn origins_for_range(&self, range: Range<usize>) -> Vec<OriginRun> {
        let mut result = Vec::new();
        for run in &self.origins {
            let start = run.logical_start as usize;
            let end = start + run.logical_len as usize;
            let overlap_start = start.max(range.start);
            let overlap_end = end.min(range.end);
            if overlap_start >= overlap_end {
                continue;
            }
            let mut clipped = run.clone();
            let left = overlap_start - start;
            clipped.logical_start =
                u32::try_from(overlap_start - range.start).expect("logical range below u32");
            clipped.logical_len =
                u32::try_from(overlap_end - overlap_start).expect("logical range below u32");
            if clipped.transform == OriginTransform::Identity
                && let Some(source) = &mut clipped.source
            {
                source.start += u32::try_from(left).expect("origin clip below u32");
                source.end = source.start + clipped.logical_len;
            }
            result.push(clipped);
        }
        result
    }

    pub fn transformed_slice(
        &self,
        range: Range<usize>,
        logical: String,
        transform: OriginTransform,
    ) -> Self {
        let source = self
            .origins_for_range(range)
            .into_iter()
            .find_map(|run| run.source);
        let mut result = Self::default();
        result.logical = logical;
        result.origins.push(OriginRun {
            logical_start: 0,
            logical_len: result
                .logical
                .len()
                .try_into()
                .expect("logical leaf below u32"),
            source,
            transform,
        });
        result
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDocument {
    pub leaves: Vec<CoverageLeaf>,
    leaf_indices: HashMap<u64, usize>,
}

impl SourceDocument {
    pub fn new(source: &str) -> Self {
        let mut leaves = Vec::new();
        let bytes = source.as_bytes();
        let mut start = 0;
        while start < bytes.len() {
            let mut end = start;
            while end < bytes.len() && !matches!(bytes[end], b'\r' | b'\n') {
                end += 1;
            }
            if end < bytes.len() {
                if bytes[end] == b'\r' && bytes.get(end + 1) == Some(&b'\n') {
                    end += 2;
                } else {
                    end += 1;
                }
            }
            leaves.push(CoverageLeaf {
                id: u64::try_from(leaves.len() + 1).expect("coverage leaf id"),
                absolute_start: start,
                text: source[start..end].to_owned(),
            });
            start = end;
        }
        Self::from_leaves(leaves)
    }

    #[must_use]
    pub fn from_leaves(leaves: Vec<CoverageLeaf>) -> Self {
        let mut leaf_indices = HashMap::with_capacity(leaves.len());
        for (index, leaf) in leaves.iter().enumerate() {
            assert!(
                leaf_indices.insert(leaf.id, index).is_none(),
                "coverage leaf ids must be unique"
            );
        }
        Self {
            leaves,
            leaf_indices,
        }
    }

    #[must_use]
    pub fn leaf(&self, id: u64) -> Option<&CoverageLeaf> {
        self.leaf_indices
            .get(&id)
            .and_then(|index| self.leaves.get(*index))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionReadError {
    InvalidRange,
    MissingCoverageLeaf(u64),
    InvalidCoverageRange,
    OriginGap,
    UnsupportedTransform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalChunk<'a> {
    Borrowed(&'a str),
    Spaces(usize),
    Newline,
}

/// Revision-borrowed cursor over an owned logical string or coverage-relative
/// raw block runs. Construction and each `next_chunk` call copy zero payload
/// bytes.
#[derive(Debug)]
pub struct LogicalProjectionCursor<'a> {
    content: &'a LeafContent,
    source: &'a SourceDocument,
    projection: LogicalProjection,
    next_origin: usize,
    logical_cursor: u32,
    owned_emitted: bool,
    suffix_emitted: bool,
}

impl LeafContent {
    pub fn projection_cursor<'a>(
        &'a self,
        source: &'a SourceDocument,
        projection: LogicalProjection,
    ) -> Result<LogicalProjectionCursor<'a>, ProjectionReadError> {
        let logical_len =
            u32::try_from(self.logical_len()).map_err(|_| ProjectionReadError::InvalidRange)?;
        if projection.start > projection.end || projection.end > logical_len {
            return Err(ProjectionReadError::InvalidRange);
        }
        if self.source_backed.is_none()
            && (self
                .logical
                .get(projection.start as usize..projection.end as usize))
            .is_none()
        {
            return Err(ProjectionReadError::InvalidRange);
        }
        let next_origin = if self.source_backed.is_some() {
            self.origins.partition_point(|run| {
                run.logical_start.saturating_add(run.logical_len) <= projection.start
            })
        } else {
            0
        };
        Ok(LogicalProjectionCursor {
            content: self,
            source,
            projection,
            next_origin,
            logical_cursor: projection.start,
            owned_emitted: false,
            suffix_emitted: false,
        })
    }

    pub fn materialize_projection(
        &self,
        source: &SourceDocument,
        projection: LogicalProjection,
    ) -> Result<String, ProjectionReadError> {
        let mut output = String::with_capacity(projection.len() as usize);
        let mut cursor = self.projection_cursor(source, projection)?;
        while let Some(chunk) = cursor.next_chunk()? {
            match chunk {
                LogicalChunk::Borrowed(text) => output.push_str(text),
                LogicalChunk::Spaces(count) => output.extend(std::iter::repeat_n(' ', count)),
                LogicalChunk::Newline => output.push('\n'),
            }
        }
        Ok(output)
    }
}

impl<'a> LogicalProjectionCursor<'a> {
    pub fn next_chunk(&mut self) -> Result<Option<LogicalChunk<'a>>, ProjectionReadError> {
        if self.content.source_backed.is_none() {
            if !self.owned_emitted && self.projection.start < self.projection.end {
                self.owned_emitted = true;
                let text = self
                    .content
                    .logical
                    .get(self.projection.start as usize..self.projection.end as usize)
                    .ok_or(ProjectionReadError::InvalidRange)?;
                return Ok(Some(LogicalChunk::Borrowed(text)));
            }
            return self.next_suffix();
        }

        while self.logical_cursor < self.projection.end {
            let Some(run) = self.content.origins.get(self.next_origin) else {
                return Err(ProjectionReadError::OriginGap);
            };
            let run_start = run.logical_start;
            let run_end = run_start
                .checked_add(run.logical_len)
                .ok_or(ProjectionReadError::InvalidRange)?;
            if run_end <= self.logical_cursor {
                self.next_origin += 1;
                continue;
            }
            if run_start > self.logical_cursor {
                return Err(ProjectionReadError::OriginGap);
            }
            let overlap_end = run_end.min(self.projection.end);
            let overlap_start = self.logical_cursor;
            let overlap_len = overlap_end - overlap_start;
            self.logical_cursor = overlap_end;
            if overlap_end == run_end {
                self.next_origin += 1;
            }
            return match run.transform {
                OriginTransform::Identity => {
                    let source = run
                        .source
                        .as_ref()
                        .ok_or(ProjectionReadError::InvalidCoverageRange)?;
                    if source.end < source.start || source.end - source.start != run.logical_len {
                        return Err(ProjectionReadError::InvalidCoverageRange);
                    }
                    let leaf = self
                        .source
                        .leaf(source.leaf_id)
                        .ok_or(ProjectionReadError::MissingCoverageLeaf(source.leaf_id))?;
                    let relative = overlap_start - run_start;
                    let start = source
                        .start
                        .checked_add(relative)
                        .ok_or(ProjectionReadError::InvalidCoverageRange)?;
                    let end = start
                        .checked_add(overlap_len)
                        .ok_or(ProjectionReadError::InvalidCoverageRange)?;
                    let text = leaf
                        .text
                        .get(start as usize..end as usize)
                        .ok_or(ProjectionReadError::InvalidCoverageRange)?;
                    Ok(Some(LogicalChunk::Borrowed(text)))
                }
                OriginTransform::TabExpansion => {
                    Ok(Some(LogicalChunk::Spaces(overlap_len as usize)))
                }
                OriginTransform::TrimAndUnescapePipes
                | OriginTransform::EntityAndBackslashNormalization
                | OriginTransform::Synthetic => Err(ProjectionReadError::UnsupportedTransform),
            };
        }
        self.next_suffix()
    }

    fn next_suffix(&mut self) -> Result<Option<LogicalChunk<'a>>, ProjectionReadError> {
        if self.projection.append_newline && !self.suffix_emitted {
            self.suffix_emitted = true;
            return Ok(Some(LogicalChunk::Newline));
        }
        Ok(None)
    }
}
