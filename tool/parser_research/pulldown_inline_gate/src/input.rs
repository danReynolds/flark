use std::ops::Range;
use std::sync::Arc;

/// A source-backed or synthetic part of a logical inline leaf.
///
/// Block parsing can remove container prefixes while preserving their source
/// offsets, and can insert the logical newline/space mandated by `CommonMark`
/// without allocating a flattened leaf string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Segment {
    Source(Range<usize>),
    VirtualNewline {
        anchor: usize,
    },
    /// One canonical logical LF with an authenticated physical line-ending
    /// scalar count (one for LF/CR, two for CRLF).
    ProjectedLineEnding {
        anchor: usize,
        raw_codepoints: u8,
    },
    VirtualSpaces {
        anchor: usize,
        count: usize,
    },
    /// Logical spaces projected from one physical tab. The first space carries
    /// the source scalar delta; continuation spaces carry zero.
    ProjectedTab {
        anchor: usize,
        spaces: usize,
    },
}

#[derive(Clone, Debug)]
struct LocatedSegment {
    logical: Range<usize>,
    segment: Segment,
}

/// Segmented logical input for one block leaf.
#[derive(Clone, Debug)]
pub struct LogicalLeaf {
    source: Arc<str>,
    segments: Arc<[LocatedSegment]>,
    logical_len: usize,
}

impl LogicalLeaf {
    #[must_use]
    pub fn contiguous(source: impl Into<Arc<str>>) -> Self {
        let source = source.into();
        let len = source.len();
        Self {
            source,
            segments: vec![LocatedSegment {
                logical: 0..len,
                segment: Segment::Source(0..len),
            }]
            .into(),
            logical_len: len,
        }
    }

    /// Builds a leaf without copying source text.
    ///
    /// Source segment boundaries must be UTF-8 boundaries. Empty segments are
    /// discarded so lookup always makes progress.
    ///
    /// # Errors
    ///
    /// Returns an error for out-of-bounds, overlapping, non-monotonic, or
    /// non-UTF-8 source boundaries, invalid virtual anchors, or length overflow.
    pub fn segmented(
        source: impl Into<Arc<str>>,
        segments: Vec<Segment>,
    ) -> Result<Self, InputError> {
        let source = source.into();
        let mut logical_len = 0usize;
        let mut physical_floor = 0usize;
        let mut located = Vec::with_capacity(segments.len());
        for segment in segments {
            let len = match &segment {
                Segment::Source(range) => {
                    if range.start > range.end
                        || range.end > source.len()
                        || !source.is_char_boundary(range.start)
                        || !source.is_char_boundary(range.end)
                    {
                        return Err(InputError::InvalidSourceRange(range.clone()));
                    }
                    if range.start < physical_floor {
                        return Err(InputError::NonMonotonicSource(range.clone()));
                    }
                    physical_floor = range.end;
                    range.len()
                }
                Segment::VirtualNewline { anchor } => {
                    if *anchor > source.len() || *anchor < physical_floor {
                        return Err(InputError::InvalidAnchor(*anchor));
                    }
                    physical_floor = *anchor;
                    1
                }
                Segment::ProjectedLineEnding {
                    anchor,
                    raw_codepoints,
                } => {
                    if *anchor > source.len()
                        || *anchor < physical_floor
                        || !matches!(raw_codepoints, 1 | 2)
                    {
                        return Err(InputError::InvalidAnchor(*anchor));
                    }
                    physical_floor = *anchor;
                    1
                }
                Segment::VirtualSpaces { anchor, count } => {
                    if *anchor > source.len() || *anchor < physical_floor {
                        return Err(InputError::InvalidAnchor(*anchor));
                    }
                    physical_floor = *anchor;
                    *count
                }
                Segment::ProjectedTab { anchor, spaces } => {
                    if *anchor > source.len() || *anchor < physical_floor || *spaces == 0 {
                        return Err(InputError::InvalidAnchor(*anchor));
                    }
                    physical_floor = *anchor;
                    *spaces
                }
            };
            if len == 0 {
                continue;
            }
            let end = logical_len
                .checked_add(len)
                .ok_or(InputError::LogicalLengthOverflow)?;
            located.push(LocatedSegment {
                logical: logical_len..end,
                segment,
            });
            logical_len = end;
        }
        Ok(Self {
            source,
            segments: located.into(),
            logical_len,
        })
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.logical_len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.logical_len == 0
    }

    #[must_use]
    pub fn source_len(&self) -> usize {
        self.source.len()
    }

    #[must_use]
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    #[must_use]
    pub fn byte_at(&self, logical: usize) -> Option<u8> {
        let located = self.locate(logical)?;
        let offset = logical - located.logical.start;
        match &located.segment {
            Segment::Source(range) => Some(self.source.as_bytes()[range.start + offset]),
            Segment::VirtualNewline { .. } | Segment::ProjectedLineEnding { .. } => Some(b'\n'),
            Segment::VirtualSpaces { .. } | Segment::ProjectedTab { .. } => Some(b' '),
        }
    }

    /// Returns the scalar beginning at `logical`, including across segment
    /// boundaries. The parser only creates source segments at UTF-8 boundaries.
    #[must_use]
    pub fn char_at(&self, logical: usize) -> Option<char> {
        if logical >= self.logical_len {
            return None;
        }
        let mut bytes = [0u8; 4];
        let mut len = 0;
        while len < bytes.len() && logical + len < self.logical_len {
            bytes[len] = self.byte_at(logical + len)?;
            len += 1;
            if let Ok(text) = std::str::from_utf8(&bytes[..len]) {
                if let Some(ch) = text.chars().next() {
                    if ch.len_utf8() == len {
                        return Some(ch);
                    }
                }
            }
        }
        None
    }

    #[must_use]
    pub fn char_before(&self, logical: usize) -> Option<char> {
        if logical == 0 || logical > self.logical_len {
            return None;
        }
        let mut start = logical - 1;
        for _ in 0..3 {
            if self
                .char_at(start)
                .is_some_and(|ch| start + ch.len_utf8() == logical)
            {
                return self.char_at(start);
            }
            if start == 0 {
                break;
            }
            start -= 1;
        }
        self.char_at(start)
            .filter(|ch| start + ch.len_utf8() == logical)
    }

    /// Raw source scalar contribution for the logical scalar beginning here.
    #[must_use]
    pub fn raw_codepoint_contribution_at(&self, logical: usize) -> Option<u8> {
        let located = self.locate(logical)?;
        let local_offset = logical - located.logical.start;
        Some(match located.segment {
            Segment::ProjectedLineEnding { raw_codepoints, .. } => raw_codepoints,
            Segment::ProjectedTab { .. } => u8::from(local_offset == 0),
            Segment::Source(_) => 1,
            Segment::VirtualNewline { .. } | Segment::VirtualSpaces { .. } => 0,
        })
    }

    /// Maps a logical range back to its exact physical source fragments.
    /// Synthetic bytes intentionally produce no physical span.
    #[must_use]
    pub fn source_spans(&self, logical: Range<usize>) -> Vec<Range<usize>> {
        let start = logical.start.min(self.logical_len);
        let end = logical.end.min(self.logical_len).max(start);
        let mut spans: Vec<Range<usize>> = Vec::new();
        for located in self.segments.iter() {
            if located.logical.end <= start {
                continue;
            }
            if located.logical.start >= end {
                break;
            }
            if let Segment::Source(source_range) = &located.segment {
                let local_start = start.max(located.logical.start) - located.logical.start;
                let local_end = end.min(located.logical.end) - located.logical.start;
                let physical = (source_range.start + local_start)..(source_range.start + local_end);
                if let Some(previous) = spans.last_mut() {
                    if previous.end == physical.start {
                        previous.end = physical.end;
                        continue;
                    }
                }
                spans.push(physical);
            }
        }
        spans
    }

    #[must_use]
    pub fn logical_text(&self, range: Range<usize>) -> String {
        let end = range.end.min(self.logical_len);
        let bytes: Vec<u8> = (range.start.min(end)..end)
            .filter_map(|offset| self.byte_at(offset))
            .collect();
        String::from_utf8(bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
    }

    fn locate(&self, logical: usize) -> Option<&LocatedSegment> {
        if logical >= self.logical_len {
            return None;
        }
        if self.segments.len() == 1 {
            return self.segments.first();
        }
        let index = self
            .segments
            .partition_point(|segment| segment.logical.end <= logical);
        self.segments.get(index)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputError {
    InvalidSourceRange(Range<usize>),
    NonMonotonicSource(Range<usize>),
    InvalidAnchor(usize),
    LogicalLengthOverflow,
}

#[cfg(test)]
mod tests {
    use super::{LogicalLeaf, Segment};

    #[test]
    fn segmented_input_maps_facts_without_flattening() {
        let leaf = LogicalLeaf::segmented(
            "left> **right**",
            vec![
                Segment::Source(0..4),
                Segment::VirtualNewline { anchor: 4 },
                Segment::Source(6..15),
            ],
        )
        .unwrap();
        assert_eq!(leaf.logical_text(0..leaf.len()), "left\n**right**");
        assert_eq!(leaf.source_spans(3..8), vec![3..4, 6..9]);
        assert_eq!(leaf.char_at(4), Some('\n'));
        assert_eq!(leaf.char_before(5), Some('\n'));
    }
}
