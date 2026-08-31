use crate::source::{LeafContent, LogicalProjection, SourceBackedContent};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyntaxProfile {
    CommonMark,
    Gfm,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListType {
    Bullet,
    Ordered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListDelimiter {
    Period,
    Paren,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListData {
    pub list_type: ListType,
    pub marker_offset: usize,
    pub padding: usize,
    pub start: usize,
    pub delimiter: ListDelimiter,
    pub bullet_char: u8,
    pub tight: bool,
    /// Present only on a GFM Item whose first inline block begins with a
    /// parser-certified task marker. List containers always retain `None`.
    pub task_checked: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableData {
    pub alignments: Vec<Alignment>,
    pub num_columns: usize,
    pub num_rows: usize,
    pub num_nonempty_cells: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockKind {
    Document,
    BlockQuote,
    List(ListData),
    Item(ListData),
    CodeBlock {
        fenced: bool,
        fence_char: u8,
        fence_length: usize,
        fence_offset: usize,
        info: LogicalProjection,
        literal: LogicalProjection,
        closed: bool,
    },
    HtmlBlock {
        block_type: u8,
        literal: LogicalProjection,
    },
    Paragraph,
    Heading {
        level: u8,
        setext: bool,
        closed: bool,
    },
    ThematicBreak,
    Table(TableData),
    TableRow {
        header: bool,
    },
    TableCell,
}

impl BlockKind {
    pub fn accepts_lines(&self) -> bool {
        matches!(
            self,
            Self::Paragraph | Self::Heading { .. } | Self::CodeBlock { .. }
        )
    }

    pub fn can_contain(&self, child: &Self) -> bool {
        match self {
            Self::Document | Self::BlockQuote | Self::Item(_) => !matches!(
                child,
                Self::Item(_) | Self::TableRow { .. } | Self::TableCell
            ),
            Self::List(_) => matches!(child, Self::Item(_)),
            Self::Table(_) => matches!(child, Self::TableRow { .. }),
            Self::TableRow { .. } => matches!(child, Self::TableCell),
            Self::Paragraph | Self::Heading { .. } | Self::TableCell => false,
            Self::CodeBlock { .. } | Self::HtmlBlock { .. } | Self::ThematicBreak => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

impl NodeId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Constant-size summary of an ordered, already-closed child prefix.
///
/// A resumed parser keeps this fold on an open frame instead of retaining the
/// historical child nodes.  The final child is kept separate because
/// CommonMark list tightness treats the final item and its final child
/// differently from every preceding sibling.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ChildSequenceFold {
    pub had_child: bool,
    pub any_nonlast_child_ends_blank: bool,
    pub last_child_ends_blank: bool,
    pub list_loose_before_last: bool,
    pub last_item_loose_if_nonlast: bool,
    pub last_item_loose_if_last: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClosedChildSummary {
    pub ends_blank: bool,
    pub item_loose_if_nonlast: bool,
    pub item_loose_if_last: bool,
}

impl ChildSequenceFold {
    pub fn push(&mut self, child: ClosedChildSummary) {
        if self.had_child {
            self.any_nonlast_child_ends_blank |= self.last_child_ends_blank;
            self.list_loose_before_last |= self.last_item_loose_if_nonlast;
        }
        self.had_child = true;
        self.last_child_ends_blank = child.ends_blank;
        self.last_item_loose_if_nonlast = child.item_loose_if_nonlast;
        self.last_item_loose_if_last = child.item_loose_if_last;
    }

    /// Apply the parser's `last_child.last_line_blank = true` operation when
    /// that child has already been folded out of transient scratch.
    pub fn mark_last_child_line_blank(&mut self) {
        if !self.had_child {
            return;
        }
        self.last_child_ends_blank = true;
        // Only an Item uses these fields.  For all other child kinds they are
        // ignored by the parent's list fold.
        self.last_item_loose_if_nonlast = true;
    }

    pub fn list_is_tight(&self) -> bool {
        !(self.list_loose_before_last || self.last_item_loose_if_last)
    }

    /// Compose summaries for two adjacent child ranges without visiting the
    /// children in either range.
    ///
    /// This is the output-side operation needed when an edited prefix is
    /// spliced in front of an unchanged suffix. It is associative because the
    /// result is exactly the summary produced by pushing the concatenated
    /// child sequence.
    #[must_use]
    pub const fn followed_by(self, suffix: Self) -> Self {
        if !self.had_child {
            return suffix;
        }
        if !suffix.had_child {
            return self;
        }
        Self {
            had_child: true,
            any_nonlast_child_ends_blank: self.any_nonlast_child_ends_blank
                || self.last_child_ends_blank
                || suffix.any_nonlast_child_ends_blank,
            last_child_ends_blank: suffix.last_child_ends_blank,
            list_loose_before_last: self.list_loose_before_last
                || self.last_item_loose_if_nonlast
                || suffix.list_loose_before_last,
            last_item_loose_if_nonlast: suffix.last_item_loose_if_nonlast,
            last_item_loose_if_last: suffix.last_item_loose_if_last,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockNode {
    pub id: NodeId,
    pub kind: BlockKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub open: bool,
    pub last_line_blank: bool,
    pub table_visited: bool,
    /// Exact source-missing cell count for the hostile-short-row guard.
    ///
    /// Comrak's donor-visible `TableData::num_nonempty_cells` counts the padded
    /// output width in the pinned implementation, so it cannot drive the guard
    /// without remaining permanently zero. This parser-only field preserves
    /// donor output metadata while making the intended safety transition real.
    pub table_autocompleted_cells: usize,
    pub source_start: Position,
    pub source_end: Position,
    pub content: LeafContent,
    /// Closed children that precede the transient children in `children`.
    /// Empty during a one-shot parse; populated only by continuation restore.
    pub historical_children: ChildSequenceFold,
    /// Prefix of `children` already summarized into `historical_children`.
    /// This makes close/finalize fold each direct child exactly once instead
    /// of recursively recomputing a deep just-closed subtree.
    pub folded_children: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockEvent {
    Open {
        node: NodeId,
        parent: NodeId,
    },
    AppendContent {
        node: NodeId,
        logical_start: u32,
        logical_end: u32,
        origin_start: u32,
        origin_end: u32,
        line_offsets_start: u32,
        line_offsets_end: u32,
        /// Event-time scalar fold for a source-backed raw block. Keeping this
        /// on the event prevents a later parser state from leaking into an
        /// earlier append delta when several lines are delivered together.
        source_backed: Option<SourceBackedContent>,
    },
    Promote {
        node: NodeId,
        from: &'static str,
        to: &'static str,
    },
    Close {
        node: NodeId,
    },
    /// Output-only full-tree source-position fold. A continuation parser emits
    /// this instead of reading historical output from scratch.
    RepairListSourcePositions {
        node: NodeId,
        scratch_positions: Vec<(NodeId, Position, Position)>,
    },
    Detach {
        node: NodeId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceOccurrence {
    pub normalized_label: String,
    pub url: String,
    pub title: String,
    pub origins: Vec<crate::source::OriginRun>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockTree {
    pub nodes: Vec<BlockNode>,
    pub root: NodeId,
    pub events: Vec<BlockEvent>,
}

impl BlockTree {
    pub fn new() -> Self {
        let root = NodeId(0);
        Self {
            nodes: vec![BlockNode {
                id: root,
                kind: BlockKind::Document,
                parent: None,
                children: Vec::new(),
                open: true,
                last_line_blank: false,
                table_visited: false,
                table_autocompleted_cells: 0,
                source_start: Position::new(1, 1),
                source_end: Position::new(1, 1),
                content: LeafContent::default(),
                historical_children: ChildSequenceFold::default(),
                folded_children: 0,
            }],
            root,
            events: Vec::new(),
        }
    }

    pub fn node(&self, id: NodeId) -> &BlockNode {
        &self.nodes[id.index()]
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut BlockNode {
        &mut self.nodes[id.index()]
    }

    pub fn append(&mut self, parent: NodeId, kind: BlockKind, start: Position) -> NodeId {
        let id = self.append_scratch(parent, kind, start);
        self.events.push(BlockEvent::Open { node: id, parent });
        id
    }

    /// Append parser scratch without creating the legacy node-id event stream.
    ///
    /// The direct command driver owns its own stack-shaped protocol. Keeping
    /// this operation separate prevents that path from accidentally routing
    /// through `BlockEvent` and a later tree materialization pass.
    pub(crate) fn append_scratch(
        &mut self,
        parent: NodeId,
        kind: BlockKind,
        start: Position,
    ) -> NodeId {
        let id = NodeId(u32::try_from(self.nodes.len()).expect("node id below u32"));
        self.nodes.push(BlockNode {
            id,
            kind,
            parent: Some(parent),
            children: Vec::new(),
            open: true,
            last_line_blank: false,
            table_visited: false,
            table_autocompleted_cells: 0,
            source_start: start,
            source_end: start,
            content: LeafContent::default(),
            historical_children: ChildSequenceFold::default(),
            folded_children: 0,
        });
        self.nodes[parent.index()].children.push(id);
        id
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).parent
    }

    pub fn first_child(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).children.first().copied()
    }

    pub fn has_any_child(&self, id: NodeId) -> bool {
        self.node(id).historical_children.had_child || self.first_child(id).is_some()
    }

    pub fn last_child(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).children.last().copied()
    }

    pub fn last_child_is_open(&self, id: NodeId) -> bool {
        self.last_child(id)
            .is_some_and(|child| self.node(child).open)
    }

    pub fn detach(&mut self, id: NodeId) {
        let Some(parent) = self.parent(id) else {
            return;
        };
        let index = self.nodes[parent.index()]
            .children
            .iter()
            .position(|child| *child == id)
            .expect("detached child present");
        assert!(
            index >= self.nodes[parent.index()].folded_children,
            "cannot detach a child already committed to the parent fold"
        );
        self.nodes[parent.index()].children.remove(index);
        self.nodes[id.index()].parent = None;
        self.events.push(BlockEvent::Detach { node: id });
    }

    /// Detach direct-parser scratch without producing a legacy node-id event.
    pub(crate) fn detach_scratch(&mut self, id: NodeId) {
        let Some(parent) = self.parent(id) else {
            return;
        };
        self.remove_unfolded_child(parent, id);
        self.nodes[id.index()].parent = None;
    }

    pub fn insert_after(&mut self, sibling: NodeId, node: NodeId) {
        let parent = self.parent(sibling).expect("sibling has parent");
        if let Some(old_parent) = self.parent(node) {
            self.remove_unfolded_child(old_parent, node);
        }
        let siblings = &mut self.nodes[parent.index()].children;
        let index = siblings
            .iter()
            .position(|candidate| *candidate == sibling)
            .expect("sibling present");
        siblings.insert(index + 1, node);
        self.nodes[node.index()].parent = Some(parent);
    }

    pub fn close(&mut self, id: NodeId) {
        self.close_scratch(id);
        self.events.push(BlockEvent::Close { node: id });
    }

    /// Close parser scratch without creating a legacy node-id event.
    pub(crate) fn close_scratch(&mut self, id: NodeId) {
        self.nodes[id.index()].open = false;
    }

    fn remove_unfolded_child(&mut self, parent: NodeId, child: NodeId) {
        let index = self.nodes[parent.index()]
            .children
            .iter()
            .position(|candidate| *candidate == child)
            .expect("moved child present");
        assert!(
            index >= self.nodes[parent.index()].folded_children,
            "cannot move a child already committed to the parent fold"
        );
        self.nodes[parent.index()].children.remove(index);
    }

    /// Commit one finalized direct child into its parent's constant-size fold.
    pub fn fold_finalized_child(&mut self, id: NodeId) {
        let Some(parent) = self.parent(id) else {
            return;
        };
        // Atomic table construction can leave completed sibling cells/rows
        // before the parser's last-child close path. Commit that prefix once
        // before committing the explicitly finalized child.
        self.fold_children_before(parent, Some(id));
        let next = self.nodes[parent.index()].folded_children;
        assert_eq!(
            self.nodes[parent.index()].children.get(next),
            Some(&id),
            "children must finalize in source order"
        );
        let summary = self.closed_child_summary(id);
        self.nodes[parent.index()].historical_children.push(summary);
        self.nodes[parent.index()].folded_children += 1;
    }

    /// Commit the maximal contiguous closed child prefix in source order.
    ///
    /// The direct parser can certify and close a later instantaneous sibling
    /// before an older unmatched leaf is retired. In that chronology the
    /// later close is deliberately deferred; closing the older sibling then
    /// drains both summaries in their immutable child order.
    pub fn fold_finalized_direct_child(&mut self, id: NodeId) {
        let Some(parent) = self.parent(id) else {
            return;
        };
        let index = self.nodes[parent.index()]
            .children
            .iter()
            .position(|candidate| *candidate == id)
            .expect("finalized direct child remains attached");
        let next = self.nodes[parent.index()].folded_children;
        assert!(index >= next, "direct child is not folded twice");
        assert!(!self.nodes[id.index()].open, "direct child is finalized");
        if index != next {
            return;
        }

        loop {
            let next = self.nodes[parent.index()].folded_children;
            let Some(child) = self.nodes[parent.index()].children.get(next).copied() else {
                break;
            };
            if self.nodes[child.index()].open {
                break;
            }
            let summary = self.closed_child_summary(child);
            self.nodes[parent.index()].historical_children.push(summary);
            self.nodes[parent.index()].folded_children += 1;
        }
    }

    /// Fold the direct-child prefix preceding the one retained on the open
    /// path. Table rows/cells may be completed as atomic siblings without
    /// individually passing through parser finalization; live compaction
    /// commits that prefix once here.
    pub fn fold_children_before(&mut self, parent: NodeId, retained: Option<NodeId>) {
        let target = retained.map_or(self.nodes[parent.index()].children.len(), |child| {
            self.nodes[parent.index()]
                .children
                .iter()
                .position(|candidate| *candidate == child)
                .expect("retained child belongs to parent")
        });
        while self.nodes[parent.index()].folded_children < target {
            let index = self.nodes[parent.index()].folded_children;
            let child = self.nodes[parent.index()].children[index];
            let summary = self.closed_child_summary(child);
            self.nodes[parent.index()].historical_children.push(summary);
            self.nodes[parent.index()].folded_children += 1;
        }
    }

    pub fn child_sequence_fold(&self, id: NodeId) -> ChildSequenceFold {
        let mut fold = self.node(id).historical_children;
        for child in self
            .node(id)
            .children
            .iter()
            .skip(self.node(id).folded_children)
            .copied()
        {
            fold.push(self.closed_child_summary(child));
        }
        fold
    }

    pub fn closed_child_summary(&self, id: NodeId) -> ClosedChildSummary {
        let node = self.node(id);
        let children = self.child_sequence_fold(id);
        let descends_through_last = matches!(node.kind, BlockKind::List(_) | BlockKind::Item(_));
        let ends_blank =
            node.last_line_blank || (descends_through_last && children.last_child_ends_blank);
        let (item_loose_if_nonlast, item_loose_if_last) = if matches!(node.kind, BlockKind::Item(_))
        {
            (
                node.last_line_blank
                    || children.any_nonlast_child_ends_blank
                    || children.last_child_ends_blank,
                children.any_nonlast_child_ends_blank,
            )
        } else {
            (false, false)
        };
        ClosedChildSummary {
            ends_blank,
            item_loose_if_nonlast,
            item_loose_if_last,
        }
    }

    pub fn list_is_tight(&self, id: NodeId) -> bool {
        self.child_sequence_fold(id).list_is_tight()
    }

    pub fn mark_last_child_line_blank(&mut self, id: NodeId) {
        if self.node(id).folded_children == self.node(id).children.len() {
            self.node_mut(id)
                .historical_children
                .mark_last_child_line_blank();
        } else if let Some(last_child) = self.last_child(id) {
            self.node_mut(last_child).last_line_blank = true;
        } else {
            self.node_mut(id)
                .historical_children
                .mark_last_child_line_blank();
        }
    }
}

impl Default for BlockTree {
    fn default() -> Self {
        Self::new()
    }
}
