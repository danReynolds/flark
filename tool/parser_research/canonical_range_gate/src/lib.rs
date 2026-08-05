//! Product-canonical block and coverage range adjudication.
//!
//! This gate deliberately consumes the exact Comrak-correspondent structural
//! event stream while ignoring `RepairListSourcePositions`. Those repair
//! events reproduce donor AST chronology; they are not grammar transitions.
//! Final product ranges instead come from terminal facts plus a pure subtree
//! hull, and every byte belongs to one member of a total coverage partition.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use flark_comrak_value_block_core::checkpoint::{
    CheckpointError, PhysicalLine, ResumableValueBlockParser, StructuralEvent, WriteOnlyBlockSink,
};
use flark_comrak_value_block_core::{
    BlockKind, LeafContent, Position, SourceDocument, SyntaxProfile,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanonicalCoverageKind {
    Terminal,
    Gap,
    ContainerMarker,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalCoverageSegment {
    pub owner: u64,
    pub kind: CanonicalCoverageKind,
    pub source: Range<usize>,
    pub utf16: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalNode {
    pub handle: u64,
    pub parent: Option<u64>,
    pub kind: BlockKind,
    pub source: Range<usize>,
    pub subtree_last: u64,
    pub terminal: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalDocument {
    pub source_len: usize,
    pub source_utf16_len: usize,
    pub nodes: Vec<CanonicalNode>,
    pub coverage: Vec<CanonicalCoverageSegment>,
    pub ignored_repair_events: usize,
    pub detached_nodes: usize,
}

#[derive(Clone, Debug)]
struct EventNode {
    handle: u64,
    parent: Option<u64>,
    kind: BlockKind,
    start: Position,
    end: Position,
    content: LeafContent,
    active: bool,
}

#[derive(Default)]
struct CanonicalSink {
    nodes: BTreeMap<u64, EventNode>,
    leaves: BTreeMap<u64, (usize, usize)>,
    repairs: usize,
    detached: usize,
}

impl CanonicalSink {
    fn new() -> Self {
        let mut sink = Self::default();
        sink.nodes.insert(
            0,
            EventNode {
                handle: 0,
                parent: None,
                kind: BlockKind::Document,
                start: Position::new(1, 1),
                end: Position::new(1, 1),
                content: LeafContent::default(),
                active: true,
            },
        );
        sink
    }
}

impl WriteOnlyBlockSink for CanonicalSink {
    fn emit(&mut self, event: StructuralEvent) {
        match event {
            StructuralEvent::SourceLeaf(leaf) => {
                self.leaves
                    .insert(leaf.id, (leaf.absolute_start, leaf.text.len()));
            }
            StructuralEvent::Open {
                handle,
                parent,
                state,
            } => {
                let previous = self.nodes.insert(
                    handle,
                    EventNode {
                        handle,
                        parent: Some(parent),
                        kind: state.kind,
                        start: state.source_start,
                        end: state.source_end,
                        content: state.content.unwrap_or_default(),
                        active: true,
                    },
                );
                assert!(previous.is_none(), "output handle opened twice");
            }
            StructuralEvent::Update {
                handle,
                state,
                preserve_source_positions,
            } => {
                let node = self.nodes.get_mut(&handle).expect("updated node exists");
                node.kind = state.kind;
                if !preserve_source_positions {
                    node.start = state.source_start;
                    node.end = state.source_end;
                }
                if let Some(content) = state.content {
                    node.content = content;
                }
            }
            StructuralEvent::UpdateSourcePositions {
                handle,
                source_start,
                source_end,
            } => {
                let node = self.nodes.get_mut(&handle).expect("position node exists");
                node.start = source_start;
                node.end = source_end;
            }
            StructuralEvent::Detach { handle } => {
                let node = self.nodes.get_mut(&handle).expect("detached node exists");
                if node.active {
                    node.active = false;
                    self.detached += 1;
                }
            }
            StructuralEvent::RepairListSourcePositions { .. } => {
                // Deliberate: this gate tests whether product-canonical ranges
                // can be a pure function of final ownership and ancestry.
                self.repairs += 1;
            }
            StructuralEvent::AppendContent { handle, delta } => {
                let content = &mut self
                    .nodes
                    .get_mut(&handle)
                    .expect("content node exists")
                    .content;
                assert_eq!(content.logical_len(), delta.logical_start);
                if let Some(source_backed) = delta.source_backed {
                    assert!(delta.logical.is_empty());
                    content.source_backed = Some(source_backed);
                } else {
                    assert!(content.source_backed.is_none());
                    content.logical.push_str(&delta.logical);
                }
                content.origins.extend(delta.origins);
                content.line_offsets.extend(delta.line_offsets);
            }
            StructuralEvent::DrainContentPrefix { handle, bytes } => self
                .nodes
                .get_mut(&handle)
                .expect("content node exists")
                .content
                .drain_prefix(bytes),
            StructuralEvent::Close { .. } | StructuralEvent::Reference(_) => {}
        }
    }
}

#[derive(Clone, Debug)]
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(offset + 1);
            }
        }
        Self { starts }
    }

    fn line_start(&self, line: usize, source_len: usize) -> usize {
        if line == 0 {
            return 0;
        }
        self.starts
            .get(line - 1)
            .copied()
            .unwrap_or(source_len)
            .min(source_len)
    }

    fn content_end(&self, line: usize, source: &str) -> usize {
        let source_len = source.len();
        let start = self.line_start(line, source_len);
        let next = self
            .starts
            .get(line)
            .copied()
            .unwrap_or(source_len)
            .min(source_len);
        if source.as_bytes().get(next.saturating_sub(1)) == Some(&b'\n') && next > start {
            next - 1
        } else {
            next
        }
    }

    fn start_offset(&self, position: Position, source: &str) -> usize {
        let line_start = self.line_start(position.line, source.len());
        let content_end = self.content_end(position.line, source);
        let mut offset = line_start
            .saturating_add(position.column.saturating_sub(1))
            .min(content_end);
        while offset > line_start && !source.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn end_offset(&self, position: Position, source: &str) -> Option<usize> {
        if position.line == 0 || position.column == 0 {
            return None;
        }
        let line_start = self.line_start(position.line, source.len());
        let content_end = self.content_end(position.line, source);
        let mut offset = line_start.saturating_add(position.column).min(content_end);
        while offset < content_end && !source.is_char_boundary(offset) {
            offset += 1;
        }
        Some(offset)
    }
}

#[derive(Clone, Copy, Debug)]
struct IntrinsicExtent {
    start: usize,
    end: usize,
}

fn is_terminal(kind: &BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::CodeBlock { .. }
            | BlockKind::HtmlBlock { .. }
            | BlockKind::Paragraph
            | BlockKind::Heading { .. }
            | BlockKind::ThematicBreak
            | BlockKind::TableCell
    )
}

fn origin_extent(node: &EventNode, sink: &CanonicalSink) -> Option<Range<usize>> {
    let mut start = usize::MAX;
    let mut end = 0;
    let mut found = false;
    for origin in &node.content.origins {
        let Some(source) = &origin.source else {
            continue;
        };
        let Some((absolute, leaf_len)) = sink.leaves.get(&source.leaf_id).copied() else {
            continue;
        };
        let local_start = usize::try_from(source.start).expect("u32 fits usize");
        let local_end = usize::try_from(source.end).expect("u32 fits usize");
        if local_start > local_end || local_end > leaf_len {
            continue;
        }
        start = start.min(absolute + local_start);
        end = end.max(absolute + local_end);
        found = true;
    }
    found.then_some(start..end)
}

fn trim_line_ending(source: &str, mut end: usize, floor: usize) -> usize {
    while end > floor && matches!(source.as_bytes()[end - 1], b'\r' | b'\n') {
        end -= 1;
    }
    end
}

fn intrinsic_extent(
    node: &EventNode,
    sink: &CanonicalSink,
    lines: &LineIndex,
    source: &str,
) -> IntrinsicExtent {
    if matches!(node.kind, BlockKind::Document) {
        return IntrinsicExtent {
            start: 0,
            end: source.len(),
        };
    }

    let raw_start = lines.start_offset(node.start, source);
    let raw_end = lines.end_offset(node.end, source);
    let origins = origin_extent(node, sink);

    // Paragraph content can begin after one or more detached reference
    // definitions. The surviving origin is the product-visible leaf; Comrak's
    // original paragraph start is historical parser scratch in that case.
    let start = if matches!(node.kind, BlockKind::Paragraph) {
        origins.as_ref().map_or(raw_start, |range| range.start)
    } else if matches!(node.kind, BlockKind::Heading { setext: true, .. }) {
        origins.as_ref().map_or(raw_start, |range| range.start)
    } else {
        raw_start
    };

    let end = raw_end.unwrap_or_else(|| {
        origins
            .as_ref()
            .map_or(start, |range| trim_line_ending(source, range.end, start))
    });
    IntrinsicExtent {
        start: start.min(source.len()),
        end: end.max(start).min(source.len()),
    }
}

fn canonicalize(
    handle: u64,
    children: &BTreeMap<u64, Vec<u64>>,
    intrinsic: &BTreeMap<u64, IntrinsicExtent>,
    output: &mut BTreeMap<u64, (Range<usize>, u64)>,
) -> (Range<usize>, u64) {
    let own = intrinsic[&handle];
    let mut start = own.start;
    let mut end = own.end;
    let mut subtree_last = handle;
    if let Some(child_handles) = children.get(&handle) {
        for child in child_handles {
            let (extent, last) = canonicalize(*child, children, intrinsic, output);
            start = start.min(extent.start);
            end = end.max(extent.end);
            subtree_last = last;
        }
    }
    let extent = start..end;
    output.insert(handle, (extent.clone(), subtree_last));
    (extent, subtree_last)
}

fn build_coverage(
    source: &str,
    nodes: &[CanonicalNode],
    depth: &BTreeMap<u64, usize>,
) -> Vec<CanonicalCoverageSegment> {
    if source.is_empty() {
        return Vec::new();
    }
    let mut boundaries = BTreeSet::from([0, source.len()]);
    for node in nodes {
        boundaries.insert(node.source.start);
        boundaries.insert(node.source.end);
    }
    for (offset, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            boundaries.insert(offset);
            boundaries.insert(offset + 1);
        }
    }
    let boundaries = boundaries.into_iter().collect::<Vec<_>>();
    let document = nodes
        .iter()
        .find(|node| node.parent.is_none())
        .expect("document node exists");
    let mut output: Vec<CanonicalCoverageSegment> = Vec::new();
    for pair in boundaries.windows(2) {
        let range = pair[0]..pair[1];
        if range.is_empty() {
            continue;
        }
        let owner = nodes
            .iter()
            .filter(|node| {
                node.handle != document.handle
                    && node.source.start <= range.start
                    && range.end <= node.source.end
            })
            .max_by_key(|node| (depth[&node.handle], usize::from(node.terminal)))
            .unwrap_or(document);
        let kind = if owner.handle == document.handle {
            CanonicalCoverageKind::Gap
        } else if owner.terminal {
            CanonicalCoverageKind::Terminal
        } else if source.as_bytes()[range.clone()]
            .iter()
            .all(u8::is_ascii_whitespace)
        {
            CanonicalCoverageKind::Gap
        } else {
            CanonicalCoverageKind::ContainerMarker
        };
        if let Some(previous) = output.last_mut()
            && previous.owner == owner.handle
            && previous.kind == kind
            && previous.source.end == range.start
        {
            previous.source.end = range.end;
            previous.utf16.end = source[..range.end].encode_utf16().count();
        } else {
            output.push(CanonicalCoverageSegment {
                owner: owner.handle,
                kind,
                utf16: source[..range.start].encode_utf16().count()
                    ..source[..range.end].encode_utf16().count(),
                source: range,
            });
        }
    }
    output
}

/// Parse one source with the exact block machine and derive product-canonical
/// ranges without applying or retaining donor repair chronology.
///
/// # Errors
///
/// Returns a checkpoint/parser error if the exact resumable block machine
/// rejects a line or cannot materialize its continuation state.
///
/// # Panics
///
/// Panics if the exact parser emits a malformed structural event stream, such
/// as an update for an unopened handle or a child without its active parent.
pub fn parse_canonical(
    source: &str,
    profile: SyntaxProfile,
) -> Result<CanonicalDocument, CheckpointError> {
    let source_document = SourceDocument::new(source);
    let mut parser = ResumableValueBlockParser::begin(profile);
    let mut sink = CanonicalSink::new();
    for leaf in &source_document.leaves {
        parser.push_line(
            PhysicalLine {
                coverage_leaf_id: leaf.id,
                absolute_start: leaf.absolute_start,
                text: &leaf.text,
            },
            &mut sink,
        )?;
    }
    parser.finish(&mut sink)?;

    let mut children = BTreeMap::<u64, Vec<u64>>::new();
    for node in sink.nodes.values().filter(|node| node.active) {
        if let Some(parent) = node.parent {
            children.entry(parent).or_default().push(node.handle);
        }
    }
    for child_handles in children.values_mut() {
        child_handles.sort_unstable();
    }

    let lines = LineIndex::new(source);
    let intrinsic = sink
        .nodes
        .values()
        .filter(|node| node.active)
        .map(|node| (node.handle, intrinsic_extent(node, &sink, &lines, source)))
        .collect::<BTreeMap<_, _>>();
    let mut canonical = BTreeMap::new();
    canonicalize(0, &children, &intrinsic, &mut canonical);

    let mut preorder = Vec::new();
    let mut stack = vec![0];
    while let Some(handle) = stack.pop() {
        preorder.push(handle);
        if let Some(child_handles) = children.get(&handle) {
            stack.extend(child_handles.iter().rev());
        }
    }
    let nodes = preorder
        .iter()
        .map(|handle| {
            let event = &sink.nodes[handle];
            let (source, subtree_last) = &canonical[handle];
            CanonicalNode {
                handle: *handle,
                parent: event.parent,
                kind: event.kind.clone(),
                source: source.clone(),
                subtree_last: *subtree_last,
                terminal: is_terminal(&event.kind),
            }
        })
        .collect::<Vec<_>>();
    let mut depth = BTreeMap::from([(0, 0)]);
    for node in nodes.iter().skip(1) {
        depth.insert(
            node.handle,
            depth[&node.parent.expect("non-root node has parent")] + 1,
        );
    }
    let coverage = build_coverage(source, &nodes, &depth);
    Ok(CanonicalDocument {
        source_len: source.len(),
        source_utf16_len: source.encode_utf16().count(),
        nodes,
        coverage,
        ignored_repair_events: sink.repairs,
        detached_nodes: sink.detached,
    })
}

/// Validate total coverage, UTF-8 boundaries, ancestry, and parent containment.
///
/// # Errors
///
/// Returns a diagnostic when the canonical document does not cover the exact
/// source once or a node range violates a structural invariant.
pub fn validate_canonical(source: &str, document: &CanonicalDocument) -> Result<(), String> {
    if document.source_len != source.len() {
        return Err("source length mismatch".to_owned());
    }
    if document.source_utf16_len != source.encode_utf16().count() {
        return Err("UTF-16 source length mismatch".to_owned());
    }
    let by_id = document
        .nodes
        .iter()
        .map(|node| (node.handle, node))
        .collect::<BTreeMap<_, _>>();
    for node in &document.nodes {
        if node.source.start > node.source.end || node.source.end > source.len() {
            return Err(format!("invalid node range: {node:?}"));
        }
        if !source.is_char_boundary(node.source.start) || !source.is_char_boundary(node.source.end)
        {
            return Err(format!("node range splits UTF-8: {node:?}"));
        }
        if let Some(parent) = node.parent {
            let parent = by_id
                .get(&parent)
                .ok_or_else(|| format!("missing parent for {node:?}"))?;
            if parent.source.start > node.source.start || node.source.end > parent.source.end {
                return Err(format!(
                    "child {:?} escapes parent {:?}",
                    node.source, parent.source
                ));
            }
        }
    }
    if source.is_empty() {
        if !document.coverage.is_empty() {
            return Err("empty source has coverage".to_owned());
        }
        return Ok(());
    }
    let mut cursor = 0;
    let mut utf16_cursor = 0;
    for segment in &document.coverage {
        if segment.source.start != cursor || segment.source.end <= segment.source.start {
            return Err(format!("coverage gap/overlap at {cursor}: {segment:?}"));
        }
        if !by_id.contains_key(&segment.owner) {
            return Err(format!("coverage owner missing: {segment:?}"));
        }
        if segment.utf16.start != utf16_cursor
            || segment.utf16.end < segment.utf16.start
            || segment.utf16.end > document.source_utf16_len
        {
            return Err(format!(
                "UTF-16 coverage gap/overlap at {utf16_cursor}: {segment:?}"
            ));
        }
        let expected_utf16 = source[..segment.source.start].encode_utf16().count()
            ..source[..segment.source.end].encode_utf16().count();
        if segment.utf16 != expected_utf16 {
            return Err(format!(
                "UTF-16 coverage does not match bytes: {segment:?}, expected={expected_utf16:?}"
            ));
        }
        cursor = segment.source.end;
        utf16_cursor = segment.utf16.end;
    }
    if cursor != source.len() {
        return Err(format!(
            "coverage ends at {cursor}, expected {}",
            source.len()
        ));
    }
    if utf16_cursor != document.source_utf16_len {
        return Err(format!(
            "UTF-16 coverage ends at {utf16_cursor}, expected {}",
            document.source_utf16_len
        ));
    }
    Ok(())
}

/// Convert one donor `[start_line, start_column, end_line, end_column]`
/// sourcepos into the half-open byte extent used by product range validation.
#[must_use]
pub fn donor_extent(source: &str, positions: [usize; 4]) -> Range<usize> {
    let lines = LineIndex::new(source);
    let start = lines.start_offset(Position::new(positions[0], positions[1]), source);
    let end = lines
        .end_offset(Position::new(positions[2], positions[3]), source)
        .unwrap_or_else(|| lines.start_offset(Position::new(positions[2], 1), source));
    start..end.max(start)
}
