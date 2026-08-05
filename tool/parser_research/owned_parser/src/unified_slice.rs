//! Research-only, deliberately narrow architecture slice for one authoritative
//! block machine. This is not a production parser and must not be used as one.
//!
//! The same line transition emits restart state, total source coverage, container
//! ancestry, semantic leaf identity, marker ranges, and aggregate facts.  It is
//! not a second Markdown implementation and is intentionally limited to block
//! quotes, bullet/ordered list items, ATX/setext headings, a deliberately small
//! one-line-header GFM table slice, paragraphs, and blank lines.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ContainerShape {
    Quote,
    BulletList { marker: u8 },
    OrderedList { delimiter: u8, start: u64 },
    Item { continuation_indent: usize },
}

impl ContainerShape {
    fn list_compatible(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::BulletList { marker: left }, Self::BulletList { marker: right }) => {
                left == right
            }
            (
                Self::OrderedList {
                    delimiter: left, ..
                },
                Self::OrderedList {
                    delimiter: right, ..
                },
            ) => left == right,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LeafShape {
    Blank,
    Paragraph,
    Heading(u8),
    Table { columns: u16 },
    TableDelimiter { columns: u16 },
    TableRow { columns: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TableAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Frame {
    id: u64,
    shape: ContainerShape,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OpenLeaf {
    id: u64,
    shape: LeafShape,
    digest: u64,
    /// A restartable table-header candidate produced by the paragraph
    /// transition itself. The bounded slice intentionally discards it as soon
    /// as the paragraph spans more than one physical line.
    table_candidate: Option<TableHeaderCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TableHeaderCandidate {
    cells: Arc<Vec<Range<usize>>>,
    markers: Arc<Vec<Range<usize>>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MachineState {
    frames: Arc<Vec<Frame>>,
    leaf: Option<OpenLeaf>,
}

impl MachineState {
    fn semantic_eq(&self, other: &Self) -> bool {
        self.frames.len() == other.frames.len()
            && self
                .frames
                .iter()
                .zip(other.frames.iter())
                .all(|(left, right)| left.shape == right.shape)
            && match (&self.leaf, &other.leaf) {
                (None, None) => true,
                (Some(left), Some(right)) => {
                    left.shape == right.shape
                        && left.digest == right.digest
                        && left.table_candidate == right.table_candidate
                }
                _ => false,
            }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SemanticFact {
    LooseList(u64),
    PromoteLeaf {
        leaf_id: u64,
        shape: LeafShape,
        markers: Arc<Vec<Range<usize>>>,
        cells: Arc<Vec<Range<usize>>>,
        alignments: Arc<Vec<TableAlignment>>,
    },
}

impl SemanticFact {
    fn semantic_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::LooseList(_), Self::LooseList(_)) => true,
            (
                Self::PromoteLeaf {
                    shape: left_shape,
                    markers: left_markers,
                    cells: left_cells,
                    alignments: left_alignments,
                    ..
                },
                Self::PromoteLeaf {
                    shape: right_shape,
                    markers: right_markers,
                    cells: right_cells,
                    alignments: right_alignments,
                    ..
                },
            ) => {
                left_shape == right_shape
                    && left_markers == right_markers
                    && left_cells == right_cells
                    && left_alignments == right_alignments
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Chunk {
    id: u64,
    /// Source-relative ranges make an unchanged suffix reusable after byte shifts.
    content: Range<usize>,
    markers: Vec<Range<usize>>,
    cells: Vec<Range<usize>>,
    alignments: Vec<TableAlignment>,
    path: Arc<Vec<Frame>>,
    leaf_id: Option<u64>,
    leaf_shape: LeafShape,
    continues_leaf: bool,
}

impl Chunk {
    fn semantic_eq(&self, other: &Self) -> bool {
        self.content == other.content
            && self.markers == other.markers
            && self.cells == other.cells
            && self.alignments == other.alignments
            && self.leaf_shape == other.leaf_shape
            && self.continues_leaf == other.continues_leaf
            && self.path.len() == other.path.len()
            && self
                .path
                .iter()
                .zip(other.path.iter())
                .all(|(left, right)| left.shape == right.shape)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LineRecord {
    len: usize,
    hash: u64,
    state_after: MachineState,
    chunk: Chunk,
    facts: Vec<SemanticFact>,
}

impl LineRecord {
    fn semantic_eq(&self, other: &Self) -> bool {
        self.len == other.len
            && self.hash == other.hash
            && self.state_after.semantic_eq(&other.state_after)
            && self.chunk.semantic_eq(&other.chunk)
            && self.facts.len() == other.facts.len()
            && self
                .facts
                .iter()
                .zip(other.facts.iter())
                .all(|(left, right)| left.semantic_eq(right))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkView {
    pub id: u64,
    pub source: Range<usize>,
    pub content: Range<usize>,
    pub markers: Vec<Range<usize>>,
    pub cells: Vec<Range<usize>>,
    pub alignments: Vec<TableAlignment>,
    pub text: String,
    pub path: Vec<(u64, ContainerShape)>,
    pub leaf_id: Option<u64>,
    pub leaf_shape: LeafShape,
    pub continues_leaf: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnifiedDelta {
    pub reparsed_lines: usize,
    pub removed_chunk_ids: Vec<u64>,
    pub inserted_chunk_ids: Vec<u64>,
    pub reused_chunks: usize,
}

#[derive(Clone)]
pub struct UnifiedSliceDocument {
    source: String,
    records: Vec<Arc<LineRecord>>,
    next_id: u64,
    loose_lists: BTreeMap<u64, usize>,
    leaf_promotions: BTreeMap<u64, LeafPromotion>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LeafPromotion {
    shape: LeafShape,
    markers: Arc<Vec<Range<usize>>>,
    cells: Arc<Vec<Range<usize>>>,
    alignments: Arc<Vec<TableAlignment>>,
}

impl UnifiedSliceDocument {
    pub fn new(source: &str) -> Self {
        assert!(!source.contains('\r'));
        let mut next_id = 1;
        let records = parse_records(source, MachineState::default(), &mut next_id);
        let loose_lists = loose_index(&records);
        let leaf_promotions = promotion_index(&records);
        Self {
            source: source.to_owned(),
            records,
            next_id,
            loose_lists,
            leaf_promotions,
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn chunks(&self) -> Vec<ChunkView> {
        let mut offset = 0;
        self.records
            .iter()
            .map(|record| {
                let source = offset..offset + record.len;
                let content =
                    offset + record.chunk.content.start..offset + record.chunk.content.end;
                let promotion = record
                    .chunk
                    .leaf_id
                    .and_then(|leaf_id| self.leaf_promotions.get(&leaf_id));
                let promoted_header = promotion.filter(|_| {
                    record.chunk.leaf_shape == LeafShape::Paragraph && !record.chunk.continues_leaf
                });
                let leaf_shape = if record.chunk.leaf_shape == LeafShape::Paragraph {
                    promotion.map_or(record.chunk.leaf_shape, |value| value.shape)
                } else {
                    record.chunk.leaf_shape
                };
                let cells = promoted_header
                    .map_or(record.chunk.cells.as_slice(), |value| {
                        value.cells.as_slice()
                    })
                    .iter()
                    .map(|range| offset + range.start..offset + range.end)
                    .collect();
                let alignments = promoted_header.map_or_else(
                    || record.chunk.alignments.clone(),
                    |value| value.alignments.as_ref().clone(),
                );
                let view = ChunkView {
                    id: record.chunk.id,
                    source,
                    text: self.source[content.clone()].to_owned(),
                    content,
                    markers: promoted_header
                        .map_or(record.chunk.markers.as_slice(), |value| {
                            value.markers.as_slice()
                        })
                        .iter()
                        .map(|range| offset + range.start..offset + range.end)
                        .collect(),
                    cells,
                    alignments,
                    path: record
                        .chunk
                        .path
                        .iter()
                        .map(|frame| (frame.id, frame.shape.clone()))
                        .collect(),
                    leaf_id: record.chunk.leaf_id,
                    leaf_shape,
                    continues_leaf: record.chunk.continues_leaf,
                };
                offset += record.len;
                view
            })
            .collect()
    }

    pub fn is_list_loose(&self, id: u64) -> bool {
        self.loose_lists.get(&id).is_some_and(|count| *count > 0)
    }

    pub fn apply_edit(&mut self, start: usize, end: usize, replacement: &str) -> UnifiedDelta {
        assert!(start <= end && end <= self.source.len());
        assert!(self.source.is_char_boundary(start) && self.source.is_char_boundary(end));
        assert!(!replacement.contains('\r'));

        let old_boundaries = boundaries(&self.records);
        let restart_index = self
            .records
            .iter()
            .scan(0usize, |offset, record| {
                *offset += record.len;
                Some(*offset)
            })
            .position(|line_end| line_end > start)
            .unwrap_or(self.records.len());
        let restart_offset = old_boundaries[restart_index];
        let mut state = restart_index
            .checked_sub(1)
            .map_or_else(MachineState::default, |index| {
                self.records[index].state_after.clone()
            });

        let mut edited = self.source.clone();
        edited.replace_range(start..end, replacement);
        let new_edit_end = start + replacement.len();
        let byte_delta = replacement.len() as isize - (end - start) as isize;
        let mut offset = restart_offset;
        let mut inserted = Vec::<Arc<LineRecord>>::new();
        let mut convergence_old_index = None;
        let mut id_map = HashMap::new();

        while offset < edited.len() {
            let line_end = line_end(&edited, offset);
            let line = &edited[offset..line_end];
            let record = Arc::new(step_line(&state, line, &mut self.next_id));
            state = record.state_after.clone();
            inserted.push(record.clone());
            offset = line_end;

            if offset < new_edit_end {
                continue;
            }
            let Some(mapped) = offset.checked_add_signed(-byte_delta) else {
                continue;
            };
            let Ok(boundary) = old_boundaries.binary_search(&mapped) else {
                continue;
            };
            if boundary == 0 {
                continue;
            }
            let candidate = boundary - 1;
            if candidate < restart_index || !record.semantic_eq(&self.records[candidate]) {
                continue;
            }
            collect_id_alignment(
                &record.state_after,
                &self.records[candidate].state_after,
                &mut id_map,
            );
            inserted.pop();
            convergence_old_index = Some(candidate);
            break;
        }

        let old_end = convergence_old_index.unwrap_or(self.records.len());
        if convergence_old_index.is_none() && offset == edited.len() {
            // EOF is an exact structural boundary even when there is no matching
            // old record to adopt.
        }
        for record in &mut inserted {
            remap_record(Arc::make_mut(record), &id_map);
        }

        let removed_chunk_ids = self.records[restart_index..old_end]
            .iter()
            .map(|record| record.chunk.id)
            .collect::<Vec<_>>();
        let inserted_chunk_ids = inserted
            .iter()
            .map(|record| record.chunk.id)
            .collect::<Vec<_>>();
        let reparsed_lines = inserted.len() + convergence_old_index.is_some() as usize;
        let reused_chunks = restart_index + self.records.len().saturating_sub(old_end);
        let mut records =
            Vec::with_capacity(restart_index + inserted.len() + self.records.len() - old_end);
        records.extend(self.records[..restart_index].iter().cloned());
        records.extend(inserted);
        records.extend(self.records[old_end..].iter().cloned());
        assert_eq!(
            records.iter().map(|record| record.len).sum::<usize>(),
            edited.len()
        );

        self.source = edited;
        self.records = records;
        self.loose_lists = loose_index(&self.records);
        self.leaf_promotions = promotion_index(&self.records);
        UnifiedDelta {
            reparsed_lines,
            removed_chunk_ids,
            inserted_chunk_ids,
            reused_chunks,
        }
    }

    pub fn assert_clean_oracle(&self) {
        let mut next_id = 1;
        let clean = parse_records(&self.source, MachineState::default(), &mut next_id);
        assert_eq!(self.records.len(), clean.len());
        for (incremental, clean) in self.records.iter().zip(clean) {
            assert!(
                incremental.semantic_eq(&clean),
                "incremental={incremental:?}\nclean={clean:?}"
            );
        }
        let current_loose = self
            .loose_lists
            .values()
            .copied()
            .filter(|count| *count > 0)
            .count();
        let clean_loose = loose_index(&clean_records_without_ids(&self.source))
            .values()
            .copied()
            .filter(|count| *count > 0)
            .count();
        assert_eq!(current_loose, clean_loose);
        let clean_promotions = promotion_index(&clean_records_without_ids(&self.source));
        let current_shapes = self
            .leaf_promotions
            .values()
            .map(|promotion| {
                (
                    promotion.shape,
                    promotion.markers.as_ref().clone(),
                    promotion.cells.as_ref().clone(),
                    promotion.alignments.as_ref().clone(),
                )
            })
            .collect::<Vec<_>>();
        let clean_shapes = clean_promotions
            .values()
            .map(|promotion| {
                (
                    promotion.shape,
                    promotion.markers.as_ref().clone(),
                    promotion.cells.as_ref().clone(),
                    promotion.alignments.as_ref().clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(current_shapes, clean_shapes);
    }
}

fn clean_records_without_ids(source: &str) -> Vec<Arc<LineRecord>> {
    let mut next_id = 1;
    parse_records(source, MachineState::default(), &mut next_id)
}

fn parse_records(source: &str, mut state: MachineState, next_id: &mut u64) -> Vec<Arc<LineRecord>> {
    let mut records = Vec::new();
    let mut offset = 0;
    while offset < source.len() {
        let end = line_end(source, offset);
        let record = Arc::new(step_line(&state, &source[offset..end], next_id));
        state = record.state_after.clone();
        records.push(record);
        offset = end;
    }
    records
}

fn step_line(state_before: &MachineState, line: &str, next_id: &mut u64) -> LineRecord {
    let content_end = line.strip_suffix('\n').map_or(line.len(), str::len);
    let content = &line[..content_end];
    let mut state = state_before.clone();
    let mut cursor = 0;
    let mut matched = 0;
    for frame in state.frames.iter() {
        match &frame.shape {
            ContainerShape::Quote => {
                let Some((_, end)) = quote_marker(&content[cursor..]) else {
                    break;
                };
                cursor += end;
                matched += 1;
            }
            ContainerShape::BulletList { .. } | ContainerShape::OrderedList { .. } => {
                matched += 1;
            }
            ContainerShape::Item {
                continuation_indent,
            } => {
                let rest = &content[cursor..];
                if blank(rest) {
                    matched += 1;
                } else if leading_spaces(rest) >= *continuation_indent {
                    cursor += *continuation_indent;
                    matched += 1;
                } else {
                    break;
                }
            }
        }
    }

    let unmatched = matched < state.frames.len();
    let rest = &content[cursor..];
    let lazy = unmatched
        && state
            .leaf
            .as_ref()
            .is_some_and(|leaf| leaf.shape == LeafShape::Paragraph)
        && !blank(rest)
        && !starts_block(rest);
    if lazy {
        let start = cursor + leading_spaces(rest).min(3);
        let leaf = state.leaf.as_mut().unwrap();
        leaf.digest = digest_extend(leaf.digest, b"\n");
        leaf.digest = digest_extend(leaf.digest, &content.as_bytes()[start..]);
        leaf.table_candidate = None;
        let leaf_id = leaf.id;
        return make_record(
            line,
            state,
            Chunk {
                id: take_id(next_id),
                content: start..content_end,
                markers: Vec::new(),
                cells: Vec::new(),
                alignments: Vec::new(),
                path: state_before.frames.clone(),
                leaf_id: Some(leaf_id),
                leaf_shape: LeafShape::Paragraph,
                continues_leaf: true,
            },
            Vec::new(),
        );
    }

    if unmatched {
        Arc::make_mut(&mut state.frames).truncate(matched);
        state.leaf = None;
    }

    if !unmatched {
        let open_shape = state.leaf.as_ref().map(|leaf| leaf.shape);
        if open_shape == Some(LeafShape::Paragraph) {
            if let Some(delimiter) = table_delimiter(&content[cursor..]) {
                let candidate = state
                    .leaf
                    .as_ref()
                    .and_then(|leaf| leaf.table_candidate.clone());
                if let Some(candidate) =
                    candidate.filter(|value| value.cells.len() == delimiter.alignments.len())
                {
                    let columns = u16::try_from(candidate.cells.len()).unwrap_or(u16::MAX);
                    let leaf = state.leaf.as_mut().unwrap();
                    leaf.shape = LeafShape::Table { columns };
                    leaf.digest = digest_extend(leaf.digest, b"\n");
                    leaf.digest = digest_extend(leaf.digest, line.as_bytes());
                    leaf.table_candidate = None;
                    let leaf_id = leaf.id;
                    return make_record(
                        line,
                        state.clone(),
                        Chunk {
                            id: take_id(next_id),
                            content: cursor..cursor,
                            markers: translate_ranges(&delimiter.markers, cursor),
                            cells: translate_ranges(&delimiter.cells, cursor),
                            alignments: delimiter.alignments.clone(),
                            path: state.frames.clone(),
                            leaf_id: Some(leaf_id),
                            leaf_shape: LeafShape::TableDelimiter { columns },
                            continues_leaf: true,
                        },
                        vec![SemanticFact::PromoteLeaf {
                            leaf_id,
                            shape: LeafShape::Table { columns },
                            markers: candidate.markers,
                            cells: candidate.cells,
                            alignments: Arc::new(delimiter.alignments),
                        }],
                    );
                }
            }

            if let Some((level, marker)) = setext_marker(&content[cursor..]) {
                let leaf = state.leaf.as_mut().unwrap();
                leaf.shape = LeafShape::Heading(level);
                leaf.digest = digest_extend(leaf.digest, b"\n");
                leaf.digest = digest_extend(leaf.digest, line.as_bytes());
                leaf.table_candidate = None;
                let leaf_id = leaf.id;
                return make_record(
                    line,
                    state.clone(),
                    Chunk {
                        id: take_id(next_id),
                        content: cursor..cursor,
                        markers: std::iter::once(cursor + marker.start..cursor + marker.end)
                            .collect(),
                        cells: Vec::new(),
                        alignments: Vec::new(),
                        path: state.frames.clone(),
                        leaf_id: Some(leaf_id),
                        leaf_shape: LeafShape::Heading(level),
                        continues_leaf: true,
                    },
                    vec![SemanticFact::PromoteLeaf {
                        leaf_id,
                        shape: LeafShape::Heading(level),
                        markers: Arc::new(Vec::new()),
                        cells: Arc::new(Vec::new()),
                        alignments: Arc::new(Vec::new()),
                    }],
                );
            }
        } else if let Some(LeafShape::Table { columns }) = open_shape {
            if !blank(&content[cursor..]) {
                if let Some(row) = table_row(&content[cursor..], false) {
                    let leaf = state.leaf.as_mut().unwrap();
                    // Once a body row has been accepted, prior header text no
                    // longer belongs in the convergence key. The current row's
                    // hash and source-relative cells remain in this record.
                    leaf.digest = table_state_digest(columns);
                    let leaf_id = leaf.id;
                    return make_record(
                        line,
                        state.clone(),
                        Chunk {
                            id: take_id(next_id),
                            content: cursor..content_end,
                            markers: translate_ranges(&row.markers, cursor),
                            cells: translate_ranges(&row.cells, cursor),
                            alignments: Vec::new(),
                            path: state.frames.clone(),
                            leaf_id: Some(leaf_id),
                            leaf_shape: LeafShape::TableRow { columns },
                            continues_leaf: true,
                        },
                        Vec::new(),
                    );
                }
            }
            state.leaf = None;
        } else if matches!(open_shape, Some(LeafShape::Heading(_))) {
            // A setext promotion is retained for exactly one checkpoint so an
            // edit to its text cannot falsely converge on the unchanged
            // underline. The next physical line starts a fresh leaf.
            state.leaf = None;
        }
    }

    if blank(&content[cursor..]) {
        state.leaf = None;
        let facts = state
            .frames
            .iter()
            .filter(|frame| {
                matches!(
                    frame.shape,
                    ContainerShape::BulletList { .. } | ContainerShape::OrderedList { .. }
                )
            })
            .map(|frame| SemanticFact::LooseList(frame.id))
            .collect();
        return make_record(
            line,
            state.clone(),
            Chunk {
                id: take_id(next_id),
                content: cursor..cursor,
                markers: Vec::new(),
                cells: Vec::new(),
                alignments: Vec::new(),
                path: state.frames.clone(),
                leaf_id: None,
                leaf_shape: LeafShape::Blank,
                continues_leaf: false,
            },
            facts,
        );
    }

    if !unmatched
        && state
            .leaf
            .as_ref()
            .is_some_and(|leaf| leaf.shape == LeafShape::Paragraph)
        && !starts_block(&content[cursor..])
    {
        let start = cursor + leading_spaces(&content[cursor..]).min(3);
        let leaf = state.leaf.as_mut().unwrap();
        leaf.digest = digest_extend(leaf.digest, b"\n");
        leaf.digest = digest_extend(leaf.digest, &content.as_bytes()[start..]);
        leaf.table_candidate = None;
        let leaf_id = leaf.id;
        return make_record(
            line,
            state.clone(),
            Chunk {
                id: take_id(next_id),
                content: start..content_end,
                markers: Vec::new(),
                cells: Vec::new(),
                alignments: Vec::new(),
                path: state.frames.clone(),
                leaf_id: Some(leaf_id),
                leaf_shape: LeafShape::Paragraph,
                continues_leaf: true,
            },
            Vec::new(),
        );
    }

    state.leaf = None;
    let mut markers = Vec::new();
    loop {
        if let Some((start, end)) = quote_marker(&content[cursor..]) {
            markers.push(cursor + start..cursor + start + 1);
            Arc::make_mut(&mut state.frames).push(Frame {
                id: take_id(next_id),
                shape: ContainerShape::Quote,
            });
            cursor += end;
            continue;
        }
        let Some(marker) = list_marker(&content[cursor..]) else {
            break;
        };
        markers.push(cursor + marker.marker.clone().start..cursor + marker.marker.end);
        let reuse_list = state
            .frames
            .last()
            .is_some_and(|frame| frame.shape.list_compatible(&marker.list_shape));
        if !reuse_list {
            Arc::make_mut(&mut state.frames).push(Frame {
                id: take_id(next_id),
                shape: marker.list_shape,
            });
        }
        Arc::make_mut(&mut state.frames).push(Frame {
            id: take_id(next_id),
            shape: ContainerShape::Item {
                continuation_indent: marker.continuation_indent,
            },
        });
        cursor += marker.content_start;
    }
    while matches!(
        state.frames.last().map(|frame| &frame.shape),
        Some(ContainerShape::BulletList { .. } | ContainerShape::OrderedList { .. })
    ) {
        Arc::make_mut(&mut state.frames).pop();
    }

    if let Some((level, marker, text)) = atx_heading(&content[cursor..]) {
        markers.push(cursor + marker.start..cursor + marker.end);
        let id = take_id(next_id);
        return make_record(
            line,
            state.clone(),
            Chunk {
                id: take_id(next_id),
                content: cursor + text.start..cursor + text.end,
                markers,
                cells: Vec::new(),
                alignments: Vec::new(),
                path: state.frames.clone(),
                leaf_id: Some(id),
                leaf_shape: LeafShape::Heading(level),
                continues_leaf: false,
            },
            Vec::new(),
        );
    }

    let start = cursor + leading_spaces(&content[cursor..]).min(3);
    let table_candidate = table_row(&content[start..], true).map(|row| TableHeaderCandidate {
        cells: Arc::new(translate_ranges(&row.cells, start)),
        markers: Arc::new(translate_ranges(&row.markers, start)),
    });
    let leaf = OpenLeaf {
        id: take_id(next_id),
        shape: LeafShape::Paragraph,
        digest: digest_extend(DIGEST_OFFSET, &content.as_bytes()[start..]),
        table_candidate,
    };
    let leaf_id = leaf.id;
    state.leaf = Some(leaf);
    make_record(
        line,
        state.clone(),
        Chunk {
            id: take_id(next_id),
            content: start..content_end,
            markers,
            cells: Vec::new(),
            alignments: Vec::new(),
            path: state.frames.clone(),
            leaf_id: Some(leaf_id),
            leaf_shape: LeafShape::Paragraph,
            continues_leaf: false,
        },
        Vec::new(),
    )
}

fn make_record(
    line: &str,
    state_after: MachineState,
    chunk: Chunk,
    facts: Vec<SemanticFact>,
) -> LineRecord {
    LineRecord {
        len: line.len(),
        hash: digest_extend(DIGEST_OFFSET, line.as_bytes()),
        state_after,
        chunk,
        facts,
    }
}

#[derive(Debug)]
struct ListMarker {
    list_shape: ContainerShape,
    marker: Range<usize>,
    continuation_indent: usize,
    content_start: usize,
}

fn list_marker(line: &str) -> Option<ListMarker> {
    let indent = leading_spaces(line);
    if indent > 3 {
        return None;
    }
    let bytes = line.as_bytes();
    let start = indent;
    let (shape, marker_end) = if matches!(bytes.get(start), Some(b'-' | b'+' | b'*')) {
        (
            ContainerShape::BulletList {
                marker: bytes[start],
            },
            start + 1,
        )
    } else {
        let digits = bytes[start..]
            .iter()
            .take(9)
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if digits == 0 {
            return None;
        }
        let delimiter = *bytes.get(start + digits)?;
        if !matches!(delimiter, b'.' | b')') {
            return None;
        }
        let value = line[start..start + digits].parse().ok()?;
        (
            ContainerShape::OrderedList {
                delimiter,
                start: value,
            },
            start + digits + 1,
        )
    };
    if bytes
        .get(marker_end)
        .is_some_and(|byte| !matches!(byte, b' ' | b'\t'))
    {
        return None;
    }
    let spaces = bytes[marker_end..]
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    let nonblank_after = marker_end + spaces < line.len();
    let padding = if (1..=4).contains(&spaces) && nonblank_after {
        spaces
    } else {
        1
    };
    let consumed_padding = spaces.min(padding);
    Some(ListMarker {
        list_shape: shape,
        marker: start..marker_end,
        continuation_indent: indent + (marker_end - start) + padding,
        content_start: marker_end + consumed_padding,
    })
}

fn quote_marker(line: &str) -> Option<(usize, usize)> {
    let spaces = leading_spaces(line);
    if spaces > 3 || line.as_bytes().get(spaces) != Some(&b'>') {
        return None;
    }
    let mut end = spaces + 1;
    if matches!(line.as_bytes().get(end), Some(b' ' | b'\t')) {
        end += 1;
    }
    Some((spaces, end))
}

fn atx_heading(line: &str) -> Option<(u8, Range<usize>, Range<usize>)> {
    let spaces = leading_spaces(line);
    if spaces > 3 {
        return None;
    }
    let count = line.as_bytes()[spaces..]
        .iter()
        .take_while(|byte| **byte == b'#')
        .count();
    if !(1..=6).contains(&count)
        || line
            .as_bytes()
            .get(spaces + count)
            .is_some_and(|byte| !matches!(byte, b' ' | b'\t'))
    {
        return None;
    }
    let mut text_start = spaces + count;
    while matches!(line.as_bytes().get(text_start), Some(b' ' | b'\t')) {
        text_start += 1;
    }
    let text_end = line.trim_ascii_end().len();
    Some((count as u8, spaces..spaces + count, text_start..text_end))
}

fn setext_marker(line: &str) -> Option<(u8, Range<usize>)> {
    let start = leading_spaces(line);
    if start > 3 {
        return None;
    }
    let end = line.trim_ascii_end().len();
    let marker = line.get(start..end)?;
    let byte = *marker.as_bytes().first()?;
    if !matches!(byte, b'=' | b'-') || !marker.bytes().all(|candidate| candidate == byte) {
        return None;
    }
    Some((if byte == b'=' { 1 } else { 2 }, start..end))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TableRowSyntax {
    cells: Vec<Range<usize>>,
    markers: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TableDelimiterSyntax {
    cells: Vec<Range<usize>>,
    markers: Vec<Range<usize>>,
    alignments: Vec<TableAlignment>,
}

/// Deliberately bounded GFM row recognizer. It handles optional outer pipes,
/// empty cells, whitespace trimming, and backslash-escaped pipes. Full inline
/// interpretation remains a later parser milestone.
fn table_row(line: &str, require_pipe: bool) -> Option<TableRowSyntax> {
    let indent = leading_spaces(line);
    if indent > 3 {
        return None;
    }
    let end = line.trim_ascii_end().len();
    if indent == end {
        return None;
    }
    let bytes = line.as_bytes();
    let pipes = (indent..end)
        .filter(|index| bytes[*index] == b'|' && !is_escaped_pipe(bytes, *index))
        .collect::<Vec<_>>();
    if require_pipe && pipes.is_empty() {
        return None;
    }

    let leading_pipe = pipes.first().copied() == Some(indent);
    let trailing_pipe = pipes.last().copied() == Some(end - 1);
    let mut cursor = if leading_pipe { indent + 1 } else { indent };
    let mut cells = Vec::new();
    for pipe in pipes.iter().copied() {
        if leading_pipe && pipe == indent {
            continue;
        }
        if trailing_pipe && pipe == end - 1 {
            cells.push(trim_cell(line, cursor..pipe));
            cursor = end;
            break;
        }
        cells.push(trim_cell(line, cursor..pipe));
        cursor = pipe + 1;
    }
    if cursor < end || pipes.is_empty() {
        cells.push(trim_cell(line, cursor..end));
    }
    if cells.is_empty() {
        return None;
    }
    Some(TableRowSyntax {
        cells,
        markers: pipes.into_iter().map(|index| index..index + 1).collect(),
    })
}

fn table_delimiter(line: &str) -> Option<TableDelimiterSyntax> {
    let row = table_row(line, true)?;
    let mut alignments = Vec::with_capacity(row.cells.len());
    for cell in &row.cells {
        let value = line.get(cell.clone())?;
        let left = value.starts_with(':');
        let right = value.ends_with(':');
        let hyphens = value.trim_start_matches(':').trim_end_matches(':');
        if hyphens.is_empty() || !hyphens.bytes().all(|byte| byte == b'-') {
            return None;
        }
        alignments.push(match (left, right) {
            (false, false) => TableAlignment::None,
            (true, false) => TableAlignment::Left,
            (false, true) => TableAlignment::Right,
            (true, true) => TableAlignment::Center,
        });
    }
    let mut markers = row.markers;
    markers.extend(row.cells.iter().cloned());
    Some(TableDelimiterSyntax {
        cells: row.cells,
        markers,
        alignments,
    })
}

fn is_escaped_pipe(bytes: &[u8], pipe: usize) -> bool {
    let mut slash_count = 0;
    let mut cursor = pipe;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        slash_count += 1;
        cursor -= 1;
    }
    slash_count % 2 == 1
}

fn trim_cell(line: &str, range: Range<usize>) -> Range<usize> {
    let bytes = line.as_bytes();
    let mut start = range.start;
    let mut end = range.end;
    while start < end && matches!(bytes[start], b' ' | b'\t') {
        start += 1;
    }
    while end > start && matches!(bytes[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    start..end
}

fn translate_ranges(ranges: &[Range<usize>], offset: usize) -> Vec<Range<usize>> {
    ranges
        .iter()
        .map(|range| offset + range.start..offset + range.end)
        .collect()
}

fn table_state_digest(columns: u16) -> u64 {
    digest_extend(DIGEST_OFFSET, &columns.to_le_bytes())
}

fn starts_block(line: &str) -> bool {
    quote_marker(line).is_some()
        || list_marker(line).is_some()
        || atx_heading(line).is_some()
        || line.trim_start().starts_with("```")
        || line.trim_start().starts_with("~~~")
}

fn collect_id_alignment(new: &MachineState, old: &MachineState, map: &mut HashMap<u64, u64>) {
    for (new, old) in new.frames.iter().zip(old.frames.iter()) {
        map.insert(new.id, old.id);
    }
    if let (Some(new), Some(old)) = (&new.leaf, &old.leaf) {
        map.insert(new.id, old.id);
    }
}

fn remap_record(record: &mut LineRecord, map: &HashMap<u64, u64>) {
    let remap = |id: &mut u64| {
        if let Some(replacement) = map.get(id) {
            *id = *replacement;
        }
    };
    for frame in Arc::make_mut(&mut record.state_after.frames) {
        remap(&mut frame.id);
    }
    if let Some(leaf) = &mut record.state_after.leaf {
        remap(&mut leaf.id);
    }
    for frame in Arc::make_mut(&mut record.chunk.path) {
        remap(&mut frame.id);
    }
    if let Some(leaf_id) = &mut record.chunk.leaf_id {
        remap(leaf_id);
    }
    for fact in &mut record.facts {
        match fact {
            SemanticFact::LooseList(id) => remap(id),
            SemanticFact::PromoteLeaf { leaf_id, .. } => remap(leaf_id),
        }
    }
}

fn loose_index(records: &[Arc<LineRecord>]) -> BTreeMap<u64, usize> {
    let mut output = BTreeMap::new();
    for record in records {
        for fact in &record.facts {
            if let SemanticFact::LooseList(id) = fact {
                *output.entry(*id).or_default() += 1;
            }
        }
    }
    output
}

fn promotion_index(records: &[Arc<LineRecord>]) -> BTreeMap<u64, LeafPromotion> {
    let mut output = BTreeMap::new();
    for record in records {
        for fact in &record.facts {
            if let SemanticFact::PromoteLeaf {
                leaf_id,
                shape,
                markers,
                cells,
                alignments,
            } = fact
            {
                output.insert(
                    *leaf_id,
                    LeafPromotion {
                        shape: *shape,
                        markers: markers.clone(),
                        cells: cells.clone(),
                        alignments: alignments.clone(),
                    },
                );
            }
        }
    }
    output
}

fn boundaries(records: &[Arc<LineRecord>]) -> Vec<usize> {
    let mut output = Vec::with_capacity(records.len() + 1);
    output.push(0);
    for record in records {
        output.push(output.last().unwrap() + record.len);
    }
    output
}

fn line_end(source: &str, start: usize) -> usize {
    source[start..]
        .find('\n')
        .map_or(source.len(), |relative| start + relative + 1)
}

fn blank(line: &str) -> bool {
    line.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
}

fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

fn take_id(next_id: &mut u64) -> u64 {
    let id = *next_id;
    *next_id += 1;
    id
}

const DIGEST_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

fn digest_extend(mut digest: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        digest ^= *byte as u64;
        digest = digest.wrapping_mul(0x100_0000_01b3);
    }
    digest
}
