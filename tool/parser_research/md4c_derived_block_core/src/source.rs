use im::Vector;
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

pub const DEFAULT_SOURCE_LEAF_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceLeafId(pub u64);

#[derive(Clone, Debug)]
pub struct SourceLeaf {
    pub id: SourceLeafId,
    text: Arc<str>,
}

impl SourceLeaf {
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafRange {
    pub leaf: SourceLeafId,
    pub local: Range<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OriginSpan {
    pub runs: Vec<LeafRange>,
}

impl OriginSpan {
    pub fn len(&self) -> usize {
        self.runs.iter().map(|run| run.local.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|run| run.local.is_empty())
    }

    pub fn append(&mut self, other: &Self) {
        for run in &other.runs {
            if let Some(last) = self.runs.last_mut() {
                if last.leaf == run.leaf && last.local.end == run.local.start {
                    last.local.end = run.local.end;
                    continue;
                }
            }
            if !run.local.is_empty() {
                self.runs.push(run.clone());
            }
        }
    }

    pub fn slice(&self, wanted: Range<usize>) -> Self {
        assert!(wanted.start <= wanted.end && wanted.end <= self.len());
        let mut cursor = 0;
        let mut result = Self::default();
        for run in &self.runs {
            let next = cursor + run.local.len();
            let start = wanted.start.max(cursor);
            let end = wanted.end.min(next);
            if start < end {
                result.append(&Self {
                    runs: vec![LeafRange {
                        leaf: run.leaf,
                        local: run.local.start + (start - cursor)
                            ..run.local.start + (end - cursor),
                    }],
                });
            }
            cursor = next;
            if cursor >= wanted.end {
                break;
            }
        }
        result
    }
}

#[derive(Clone, Debug)]
pub struct SegmentedSource {
    leaves: Vector<Arc<SourceLeaf>>,
    next_id: u64,
    len: usize,
}

impl SegmentedSource {
    pub fn from_text(text: &str) -> Self {
        Self::from_text_with_leaf_bytes(text, DEFAULT_SOURCE_LEAF_BYTES)
    }

    pub fn from_text_with_leaf_bytes(text: &str, target: usize) -> Self {
        assert!(target > 0);
        let mut leaves = Vector::new();
        let mut start = 0;
        let mut next_id = 1;
        while start < text.len() {
            let mut end = (start + target).min(text.len());
            while end > start && !text.is_char_boundary(end) {
                end -= 1;
            }
            if end == start {
                end = text[start..]
                    .char_indices()
                    .nth(1)
                    .map_or(text.len(), |(offset, _)| start + offset);
            }
            leaves.push_back(Arc::new(SourceLeaf {
                id: SourceLeafId(next_id),
                text: Arc::from(&text[start..end]),
            }));
            next_id += 1;
            start = end;
        }
        Self {
            leaves,
            next_id,
            len: text.len(),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn materialize(&self) -> String {
        let mut result = String::with_capacity(self.len);
        for leaf in &self.leaves {
            result.push_str(leaf.text());
        }
        result
    }

    pub fn leaf_ids(&self) -> Vec<SourceLeafId> {
        self.leaves.iter().map(|leaf| leaf.id).collect()
    }

    pub fn leaf_arc(&self, id: SourceLeafId) -> Option<Arc<SourceLeaf>> {
        self.leaves.iter().find(|leaf| leaf.id == id).cloned()
    }

    fn locate(&self, absolute: usize) -> Option<(usize, usize)> {
        if absolute > self.len {
            return None;
        }
        let mut prefix = 0;
        for (index, leaf) in self.leaves.iter().enumerate() {
            let end = prefix + leaf.text.len();
            if absolute < end || (absolute == end && absolute == self.len) {
                return Some((index, absolute - prefix));
            }
            prefix = end;
        }
        (absolute == 0 && self.len == 0).then_some((0, 0))
    }

    pub fn origin_for_absolute(&self, range: Range<usize>) -> OriginSpan {
        assert!(range.start <= range.end && range.end <= self.len);
        if range.is_empty() {
            return OriginSpan::default();
        }
        let mut result = OriginSpan::default();
        let mut prefix = 0;
        for leaf in &self.leaves {
            let end = prefix + leaf.text.len();
            let start_abs = range.start.max(prefix);
            let end_abs = range.end.min(end);
            if start_abs < end_abs {
                result.append(&OriginSpan {
                    runs: vec![LeafRange {
                        leaf: leaf.id,
                        local: start_abs - prefix..end_abs - prefix,
                    }],
                });
            }
            prefix = end;
            if prefix >= range.end {
                break;
            }
        }
        result
    }

    pub fn materialize_origin(&self, origin: &OriginSpan) -> String {
        let by_id = self
            .leaves
            .iter()
            .map(|leaf| (leaf.id, leaf.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut result = String::with_capacity(origin.len());
        for run in &origin.runs {
            let leaf = by_id.get(&run.leaf).expect("origin leaf is visible");
            result.push_str(&leaf.text[run.local.clone()]);
        }
        result
    }

    pub fn bind_origin(&self, origin: &OriginSpan) -> Vec<Range<usize>> {
        let mut prefixes = BTreeMap::new();
        let mut prefix = 0;
        for leaf in &self.leaves {
            prefixes.insert(leaf.id, prefix);
            prefix += leaf.text.len();
        }
        origin
            .runs
            .iter()
            .map(|run| {
                let base = prefixes[&run.leaf];
                base + run.local.start..base + run.local.end
            })
            .collect()
    }

    pub fn line_at(&self, absolute: usize) -> Option<LineWindow> {
        if absolute >= self.len {
            return None;
        }
        let mut end = absolute;
        let text = self.materialize_range(absolute..self.len);
        let bytes = text.as_bytes();
        let content_len = bytes.iter().position(|byte| *byte == b'\n').unwrap_or(bytes.len());
        end += content_len;
        let next = if end < self.len { end + 1 } else { end };
        let scratch = self.materialize_range(absolute..next);
        Some(LineWindow {
            absolute_start: absolute,
            content_len,
            scratch,
            origin: self.origin_for_absolute(absolute..next),
        })
    }

    pub fn materialize_range(&self, range: Range<usize>) -> String {
        self.materialize_origin(&self.origin_for_absolute(range))
    }

    pub fn absolute_of_leaf(&self, id: SourceLeafId) -> Option<usize> {
        let mut prefix = 0;
        for leaf in &self.leaves {
            if leaf.id == id {
                return Some(prefix);
            }
            prefix += leaf.text.len();
        }
        None
    }

    pub fn splice(&self, range: Range<usize>, replacement: &str) -> Self {
        assert!(range.start <= range.end && range.end <= self.len);
        let before = self.materialize_range(0..range.start);
        let after = self.materialize_range(range.end..self.len);
        // First implementation deliberately preserves complete untouched
        // suffix leaves by identity. Boundary fragments become new leaves.
        let suffix_boundary = self.locate(range.end);
        let mut leaves = Vector::new();
        let mut next_id = self.next_id;
        let changed = format!("{before}{replacement}");
        let changed_source = Self::from_text_with_leaf_bytes(&changed, DEFAULT_SOURCE_LEAF_BYTES);
        for leaf in changed_source.leaves {
            leaves.push_back(Arc::new(SourceLeaf {
                id: SourceLeafId(next_id),
                text: leaf.text.clone(),
            }));
            next_id += 1;
        }
        if let Some((index, local)) = suffix_boundary {
            if index < self.leaves.len() {
                let boundary = &self.leaves[index];
                if local < boundary.text.len() {
                    leaves.push_back(Arc::new(SourceLeaf {
                        id: SourceLeafId(next_id),
                        text: Arc::from(&boundary.text[local..]),
                    }));
                    next_id += 1;
                }
                for suffix in self.leaves.iter().skip(index + 1) {
                    leaves.push_back(suffix.clone());
                }
            }
        }
        let len = range.start + replacement.len() + after.len();
        Self {
            leaves,
            next_id,
            len,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LineWindow {
    pub absolute_start: usize,
    pub content_len: usize,
    pub scratch: String,
    pub origin: OriginSpan,
}

impl LineWindow {
    pub fn content(&self) -> &str {
        &self.scratch[..self.content_len]
    }

    pub fn next_absolute(&self) -> usize {
        self.absolute_start + self.scratch.len()
    }

    pub fn origin_slice(&self, local: Range<usize>) -> OriginSpan {
        self.origin.slice(local)
    }
}
