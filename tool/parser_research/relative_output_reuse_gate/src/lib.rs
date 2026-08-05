//! Focused proof that persistent Markdown output is revision-independent.
//!
//! Crop owns source revisions only while parsing. Persistent output stores
//! page-local facts and aggregate source lengths; it contains no Crop lease,
//! root identity, absolute document offset, or borrowed source text.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use flark_integrated_parser_slice::block::{BlockJob, BlockStatus};
use flark_integrated_parser_slice::crop_source::CropSnapshotLease;

const DIGEST_BASE: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PropertyId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {
    pub byte: usize,
    pub utf16: usize,
}

impl Position {
    #[must_use]
    pub fn of(text: &str) -> Self {
        Self {
            byte: text.len(),
            utf16: text.encode_utf16().count(),
        }
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            byte: self.byte.checked_add(other.byte)?,
            utf16: self.utf16.checked_add(other.utf16)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalRange {
    pub start: Position,
    pub end: Position,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading,
    ListItem,
    FencedCode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceRole {
    Definition,
    Consumer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceOccurrence {
    pub symbol: SymbolId,
    pub local: Position,
    pub role: ReferenceRole,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockFact {
    pub kind: BlockKind,
    pub range: LocalRange,
    pub container_depth: u8,
    /// Stable property identity; list tightness is deliberately not copied here.
    pub list_property: Option<PropertyId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceAggregate {
    pub occurrences: usize,
    pub digest: u64,
    power: u64,
}

impl Default for ReferenceAggregate {
    fn default() -> Self {
        Self {
            occurrences: 0,
            digest: 0,
            power: 1,
        }
    }
}

impl ReferenceAggregate {
    fn from_occurrences(occurrences: &[ReferenceOccurrence]) -> Self {
        let mut aggregate = Self::default();
        for occurrence in occurrences {
            aggregate.digest = aggregate
                .digest
                .wrapping_mul(DIGEST_BASE)
                .wrapping_add(reference_token(*occurrence));
            aggregate.power = aggregate.power.wrapping_mul(DIGEST_BASE);
            aggregate.occurrences += 1;
        }
        aggregate
    }

    fn concat(self, right: Self) -> Self {
        Self {
            occurrences: self.occurrences + right.occurrences,
            digest: self
                .digest
                .wrapping_mul(right.power)
                .wrapping_add(right.digest),
            power: self.power.wrapping_mul(right.power),
        }
    }
}

fn reference_token(occurrence: ReferenceOccurrence) -> u64 {
    let role = match occurrence.role {
        ReferenceRole::Definition => 0x9e37_79b9_u64,
        ReferenceRole::Consumer => 0x85eb_ca6b_u64,
    };
    occurrence
        .symbol
        .0
        .wrapping_mul(0x517c_c1b7_2722_0a95)
        .rotate_left(17)
        ^ (occurrence.local.byte as u64).rotate_left(31)
        ^ (occurrence.local.utf16 as u64).rotate_left(43)
        ^ role
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocationReceipt {
    pub output_pages_allocated: usize,
    pub fact_records_allocated: usize,
    pub fact_arrays_allocated: usize,
    pub reference_records_allocated: usize,
    pub reference_arrays_allocated: usize,
    pub leaf_nodes_allocated: usize,
    pub branch_nodes_allocated: usize,
    pub tree_nodes_visited: usize,
}

impl AllocationReceipt {
    #[must_use]
    pub const fn tree_nodes_allocated(self) -> usize {
        self.leaf_nodes_allocated + self.branch_nodes_allocated
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageError {
    InvalidFactRange { fact: usize },
    InvalidReferencePosition { occurrence: usize },
}

impl fmt::Display for PageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFactRange { fact } => {
                write!(formatter, "fact {fact} is outside its coverage page")
            }
            Self::InvalidReferencePosition { occurrence } => write!(
                formatter,
                "reference occurrence {occurrence} is outside its coverage page"
            ),
        }
    }
}

impl std::error::Error for PageError {}

/// Immutable, revision-independent coverage and parsed facts.
///
/// Deliberately absent: source root, Crop lease, `Weak<CropSnapshotLease>`,
/// absolute document offset, and source slice/string.
#[derive(Debug)]
pub struct OutputPage {
    id: PageId,
    coverage: Position,
    facts: Box<[BlockFact]>,
    references: Box<[ReferenceOccurrence]>,
    reference_aggregate: ReferenceAggregate,
}

impl OutputPage {
    pub fn from_fragment(
        id: PageId,
        fragment: &str,
        facts: Vec<BlockFact>,
        references: Vec<ReferenceOccurrence>,
        receipt: &mut AllocationReceipt,
    ) -> Result<Arc<Self>, PageError> {
        let coverage = Position::of(fragment);
        Self::from_metrics(id, coverage, facts, references, receipt)
    }

    pub fn from_metrics(
        id: PageId,
        coverage: Position,
        facts: Vec<BlockFact>,
        references: Vec<ReferenceOccurrence>,
        receipt: &mut AllocationReceipt,
    ) -> Result<Arc<Self>, PageError> {
        for (index, fact) in facts.iter().enumerate() {
            if fact.range.start.byte > fact.range.end.byte
                || fact.range.start.utf16 > fact.range.end.utf16
                || fact.range.end.byte > coverage.byte
                || fact.range.end.utf16 > coverage.utf16
            {
                return Err(PageError::InvalidFactRange { fact: index });
            }
        }
        for (index, occurrence) in references.iter().enumerate() {
            if occurrence.local.byte > coverage.byte || occurrence.local.utf16 > coverage.utf16 {
                return Err(PageError::InvalidReferencePosition { occurrence: index });
            }
        }
        receipt.output_pages_allocated += 1;
        receipt.fact_records_allocated += facts.len();
        receipt.fact_arrays_allocated += usize::from(!facts.is_empty());
        receipt.reference_records_allocated += references.len();
        receipt.reference_arrays_allocated += usize::from(!references.is_empty());
        let reference_aggregate = ReferenceAggregate::from_occurrences(&references);
        Ok(Arc::new(Self {
            id,
            coverage,
            facts: facts.into_boxed_slice(),
            references: references.into_boxed_slice(),
            reference_aggregate,
        }))
    }

    #[must_use]
    pub const fn id(&self) -> PageId {
        self.id
    }

    #[must_use]
    pub const fn coverage(&self) -> Position {
        self.coverage
    }

    #[must_use]
    pub fn facts(&self) -> &[BlockFact] {
        &self.facts
    }

    #[must_use]
    pub fn references(&self) -> &[ReferenceOccurrence] {
        &self.references
    }

    #[must_use]
    pub fn semantically_eq(&self, other: &Self) -> bool {
        self.coverage == other.coverage
            && self.facts == other.facts
            && self.references == other.references
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreeSummary {
    pub pages: usize,
    pub coverage: Position,
    pub facts: usize,
    pub references: ReferenceAggregate,
    height: u16,
}

#[derive(Debug)]
enum NodeKind {
    Leaf(Arc<OutputPage>),
    Branch { left: Arc<Node>, right: Arc<Node> },
}

#[derive(Debug)]
struct Node {
    summary: TreeSummary,
    kind: NodeKind,
}

fn leaf_node(page: Arc<OutputPage>, receipt: &mut AllocationReceipt) -> Arc<Node> {
    receipt.leaf_nodes_allocated += 1;
    Arc::new(Node {
        summary: TreeSummary {
            pages: 1,
            coverage: page.coverage,
            facts: page.facts.len(),
            references: page.reference_aggregate,
            height: 1,
        },
        kind: NodeKind::Leaf(page),
    })
}

fn branch_node(left: Arc<Node>, right: Arc<Node>, receipt: &mut AllocationReceipt) -> Arc<Node> {
    receipt.branch_nodes_allocated += 1;
    Arc::new(Node {
        summary: TreeSummary {
            pages: left.summary.pages + right.summary.pages,
            coverage: left
                .summary
                .coverage
                .checked_add(right.summary.coverage)
                .expect("output coverage length overflow"),
            facts: left.summary.facts + right.summary.facts,
            references: left.summary.references.concat(right.summary.references),
            height: left.summary.height.max(right.summary.height) + 1,
        },
        kind: NodeKind::Branch { left, right },
    })
}

fn build_balanced(pages: &[Arc<OutputPage>], receipt: &mut AllocationReceipt) -> Option<Arc<Node>> {
    match pages.len() {
        0 => None,
        1 => Some(leaf_node(Arc::clone(&pages[0]), receipt)),
        len => {
            let middle = len / 2;
            let left = build_balanced(&pages[..middle], receipt).expect("non-empty left half");
            let right = build_balanced(&pages[middle..], receipt).expect("non-empty right half");
            Some(branch_node(left, right, receipt))
        }
    }
}

fn replace_prefix_node(
    node: &Arc<Node>,
    replacements: &[Arc<OutputPage>],
    receipt: &mut AllocationReceipt,
) -> Arc<Node> {
    receipt.tree_nodes_visited += 1;
    if replacements.is_empty() {
        return Arc::clone(node);
    }
    match &node.kind {
        NodeKind::Leaf(_) => {
            debug_assert_eq!(replacements.len(), 1);
            leaf_node(Arc::clone(&replacements[0]), receipt)
        }
        NodeKind::Branch { left, right } => {
            let left_pages = left.summary.pages;
            if replacements.len() <= left_pages {
                let next_left = replace_prefix_node(left, replacements, receipt);
                branch_node(next_left, Arc::clone(right), receipt)
            } else {
                let (left_replacements, right_replacements) = replacements.split_at(left_pages);
                let next_left = replace_prefix_node(left, left_replacements, receipt);
                let next_right = replace_prefix_node(right, right_replacements, receipt);
                branch_node(next_left, next_right, receipt)
            }
        }
    }
}

fn join_nodes(left: Arc<Node>, right: Arc<Node>, receipt: &mut AllocationReceipt) -> Arc<Node> {
    receipt.tree_nodes_visited += 1;
    let left_height = left.summary.height;
    let right_height = right.summary.height;
    if left_height > right_height + 1 {
        let NodeKind::Branch {
            left: outer,
            right: inner,
        } = &left.kind
        else {
            unreachable!("height difference requires a branch")
        };
        let joined = join_nodes(Arc::clone(inner), right, receipt);
        return balance_node(Arc::clone(outer), joined, receipt);
    }
    if right_height > left_height + 1 {
        let NodeKind::Branch {
            left: inner,
            right: outer,
        } = &right.kind
        else {
            unreachable!("height difference requires a branch")
        };
        let joined = join_nodes(left, Arc::clone(inner), receipt);
        return balance_node(joined, Arc::clone(outer), receipt);
    }
    branch_node(left, right, receipt)
}

fn balance_node(left: Arc<Node>, right: Arc<Node>, receipt: &mut AllocationReceipt) -> Arc<Node> {
    if left.summary.height > right.summary.height + 1 {
        let NodeKind::Branch {
            left: left_left,
            right: left_right,
        } = &left.kind
        else {
            unreachable!("unbalanced node is a branch")
        };
        if left_right.summary.height > left_left.summary.height {
            let NodeKind::Branch {
                left: pivot_left,
                right: pivot_right,
            } = &left_right.kind
            else {
                unreachable!("inner-heavy child is a branch")
            };
            let next_left = branch_node(Arc::clone(left_left), Arc::clone(pivot_left), receipt);
            let next_right = branch_node(Arc::clone(pivot_right), right, receipt);
            return branch_node(next_left, next_right, receipt);
        }
        let next_right = branch_node(Arc::clone(left_right), right, receipt);
        return branch_node(Arc::clone(left_left), next_right, receipt);
    }
    if right.summary.height > left.summary.height + 1 {
        let NodeKind::Branch {
            left: right_left,
            right: right_right,
        } = &right.kind
        else {
            unreachable!("unbalanced node is a branch")
        };
        if right_left.summary.height > right_right.summary.height {
            let NodeKind::Branch {
                left: pivot_left,
                right: pivot_right,
            } = &right_left.kind
            else {
                unreachable!("inner-heavy child is a branch")
            };
            let next_left = branch_node(left, Arc::clone(pivot_left), receipt);
            let next_right = branch_node(Arc::clone(pivot_right), Arc::clone(right_right), receipt);
            return branch_node(next_left, next_right, receipt);
        }
        let next_left = branch_node(left, Arc::clone(right_left), receipt);
        return branch_node(next_left, Arc::clone(right_right), receipt);
    }
    branch_node(left, right, receipt)
}

fn concat_roots(
    left: Option<Arc<Node>>,
    right: Option<Arc<Node>>,
    receipt: &mut AllocationReceipt,
) -> Option<Arc<Node>> {
    match (left, right) {
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) => Some(join_nodes(left, right, receipt)),
    }
}

fn split_node(
    node: &Arc<Node>,
    page_index: usize,
    receipt: &mut AllocationReceipt,
) -> (Option<Arc<Node>>, Option<Arc<Node>>) {
    receipt.tree_nodes_visited += 1;
    if page_index == 0 {
        return (None, Some(Arc::clone(node)));
    }
    if page_index == node.summary.pages {
        return (Some(Arc::clone(node)), None);
    }
    let NodeKind::Branch { left, right } = &node.kind else {
        unreachable!("a leaf only permits boundary splits")
    };
    match page_index.cmp(&left.summary.pages) {
        Ordering::Less => {
            let (prefix, middle) = split_node(left, page_index, receipt);
            (
                prefix,
                concat_roots(middle, Some(Arc::clone(right)), receipt),
            )
        }
        Ordering::Equal => (Some(Arc::clone(left)), Some(Arc::clone(right))),
        Ordering::Greater => {
            let (middle, suffix) = split_node(right, page_index - left.summary.pages, receipt);
            (
                concat_roots(Some(Arc::clone(left)), middle, receipt),
                suffix,
            )
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct OutputTree {
    root: Option<Arc<Node>>,
}

impl OutputTree {
    #[must_use]
    pub fn from_pages(pages: &[Arc<OutputPage>], receipt: &mut AllocationReceipt) -> Self {
        Self {
            root: build_balanced(pages, receipt),
        }
    }

    #[must_use]
    pub fn summary(&self) -> TreeSummary {
        self.root
            .as_ref()
            .map_or_else(TreeSummary::default, |root| root.summary)
    }

    #[must_use]
    pub fn page_count(&self) -> usize {
        self.summary().pages
    }

    #[must_use]
    pub fn height(&self) -> usize {
        usize::from(self.summary().height)
    }

    #[must_use]
    pub fn shares_root_with(&self, other: &Self) -> bool {
        match (&self.root, &other.root) {
            (None, None) => true,
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }

    /// Returns the root's right partition as an opaque identity-bearing tree.
    #[must_use]
    pub fn right_partition(&self) -> Option<Self> {
        let root = self.root.as_ref()?;
        let NodeKind::Branch { right, .. } = &root.kind else {
            return None;
        };
        Some(Self {
            root: Some(Arc::clone(right)),
        })
    }

    /// Same-cardinality fast path. It rewrites one minimal prefix path/forest.
    #[must_use]
    pub fn replace_prefix_pages(
        &self,
        replacements: &[Arc<OutputPage>],
        receipt: &mut AllocationReceipt,
    ) -> Self {
        assert!(replacements.len() <= self.page_count());
        if replacements.is_empty() {
            return self.clone();
        }
        Self {
            root: Some(replace_prefix_node(
                self.root.as_ref().expect("non-empty replacement target"),
                replacements,
                receipt,
            )),
        }
    }

    /// General persistent splice. Payload pages outside the replaced range are
    /// never copied or rebased. Balanced index paths alone are rebuilt.
    #[must_use]
    pub fn splice_pages(
        &self,
        range: Range<usize>,
        replacements: &[Arc<OutputPage>],
        receipt: &mut AllocationReceipt,
    ) -> Self {
        assert!(range.start <= range.end && range.end <= self.page_count());
        let replacement = build_balanced(replacements, receipt);
        let (prefix, tail) = match &self.root {
            Some(root) => split_node(root, range.start, receipt),
            None => (None, None),
        };
        let suffix = if let Some(tail) = tail {
            split_node(&tail, range.end - range.start, receipt).1
        } else {
            None
        };
        let prefix_and_replacement = concat_roots(prefix, replacement, receipt);
        Self {
            root: concat_roots(prefix_and_replacement, suffix, receipt),
        }
    }

    #[must_use]
    pub fn locate_page(&self, page_index: usize) -> Option<PageLocation> {
        let mut node = Arc::clone(self.root.as_ref()?);
        if page_index >= node.summary.pages {
            return None;
        }
        let mut index = page_index;
        let mut prefix = Position::default();
        let mut nodes_visited = 0;
        loop {
            nodes_visited += 1;
            match &node.kind {
                NodeKind::Leaf(page) => {
                    return Some(PageLocation {
                        page: Arc::clone(page),
                        prefix,
                        nodes_visited,
                    });
                }
                NodeKind::Branch { left, right } => {
                    if index < left.summary.pages {
                        node = Arc::clone(left);
                    } else {
                        index -= left.summary.pages;
                        prefix = prefix
                            .checked_add(left.summary.coverage)
                            .expect("output prefix length overflow");
                        node = Arc::clone(right);
                    }
                }
            }
        }
    }

    #[must_use]
    pub fn absolute_fact(&self, page_index: usize, fact_index: usize) -> Option<AbsoluteFact> {
        let location = self.locate_page(page_index)?;
        let fact = location.page.facts.get(fact_index)?.clone();
        Some(AbsoluteFact {
            page_id: location.page.id,
            kind: fact.kind,
            range: LocalRange {
                start: location.prefix.checked_add(fact.range.start)?,
                end: location.prefix.checked_add(fact.range.end)?,
            },
            nodes_visited: location.nodes_visited,
        })
    }

    #[must_use]
    pub fn pages(&self) -> PageIterator {
        PageIterator::new(self.root.clone())
    }

    #[must_use]
    pub fn semantically_eq(&self, other: &Self) -> bool {
        self.page_count() == other.page_count()
            && self
                .pages()
                .zip(other.pages())
                .all(|(left, right)| left.semantically_eq(&right))
    }
}

#[derive(Debug)]
pub struct PageLocation {
    pub page: Arc<OutputPage>,
    pub prefix: Position,
    pub nodes_visited: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbsoluteFact {
    pub page_id: PageId,
    pub kind: BlockKind,
    pub range: LocalRange,
    pub nodes_visited: usize,
}

pub struct PageIterator {
    stack: Vec<Arc<Node>>,
}

impl PageIterator {
    fn new(root: Option<Arc<Node>>) -> Self {
        let mut result = Self { stack: Vec::new() };
        if let Some(root) = root {
            result.push_left(root);
        }
        result
    }

    fn push_left(&mut self, mut node: Arc<Node>) {
        loop {
            self.stack.push(Arc::clone(&node));
            let NodeKind::Branch { left, .. } = &node.kind else {
                return;
            };
            node = Arc::clone(left);
        }
    }
}

impl Iterator for PageIterator {
    type Item = Arc<OutputPage>;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        match &node.kind {
            NodeKind::Leaf(page) => Some(Arc::clone(page)),
            NodeKind::Branch { right, .. } => {
                self.push_left(Arc::clone(right));
                self.next()
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolValue {
    pub destination: Arc<str>,
    pub title: Option<Arc<str>>,
    pub presence_generation: u64,
}

#[derive(Clone, Debug, Default)]
pub struct SymbolTable {
    values: Arc<BTreeMap<SymbolId, SymbolValue>>,
}

impl SymbolTable {
    #[must_use]
    pub fn value(&self, symbol: SymbolId) -> Option<&SymbolValue> {
        self.values.get(&symbol)
    }

    /// Prototype-only copy-on-write table update. Production needs the same
    /// indirection backed by a persistent map, not this whole-map clone.
    #[must_use]
    pub fn with_value(&self, symbol: SymbolId, value: SymbolValue) -> Self {
        let mut values = (*self.values).clone();
        values.insert(symbol, value);
        Self {
            values: Arc::new(values),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyValue {
    ListTight(bool),
}

#[derive(Clone, Debug, Default)]
pub struct PropertyTable {
    values: Arc<BTreeMap<PropertyId, PropertyValue>>,
}

impl PropertyTable {
    #[must_use]
    pub fn value(&self, property: PropertyId) -> Option<PropertyValue> {
        self.values.get(&property).copied()
    }

    /// Prototype-only copy-on-write update; page facts retain only the ID.
    #[must_use]
    pub fn with_value(&self, property: PropertyId, value: PropertyValue) -> Self {
        let mut values = (*self.values).clone();
        values.insert(property, value);
        Self {
            values: Arc::new(values),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DetachReceipt {
    pub source_bytes_materialized: usize,
    pub parsed_block_leaves: usize,
    pub retained_strong_source_leases: usize,
    pub retained_weak_source_leases: usize,
    pub allocations: AllocationReceipt,
}

#[derive(Debug)]
pub enum DetachError {
    Block(String),
    Page(PageError),
    InvalidBlockRange,
}

impl fmt::Display for DetachError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Block(error) => write!(formatter, "block parse failed: {error}"),
            Self::Page(error) => error.fmt(formatter),
            Self::InvalidBlockRange => {
                formatter.write_str("block output range was not scalar-safe")
            }
        }
    }
}

impl std::error::Error for DetachError {}

impl From<PageError> for DetachError {
    fn from(value: PageError) -> Self {
        Self::Page(value)
    }
}

#[derive(Clone, Copy)]
struct DetachedBlock {
    id: PageId,
    start: usize,
    end: usize,
    container_depth: u8,
}

/// Runs the real Crop-backed integrated block prototype, then converts its
/// revision-local leaves to detached page-local facts and drops every source
/// binding before returning.
///
/// Full source materialization is intentional instrumentation debt in this
/// focused gate; the production adapter must derive UTF-16 metrics while
/// scanning instead.
pub fn detach_crop_blocks(
    source: Arc<CropSnapshotLease>,
) -> Result<(OutputTree, DetachReceipt), DetachError> {
    let baseline_strong = Arc::strong_count(&source);
    let baseline_weak = Arc::weak_count(&source);
    let text = source.materialize();
    let mut job = BlockJob::new_crop(Arc::clone(&source));
    loop {
        let poll = job.poll(4096);
        match poll.status {
            BlockStatus::Pending => {}
            BlockStatus::Ready => break,
            BlockStatus::Failed => {
                return Err(DetachError::Block(
                    job.error()
                        .map_or_else(|| "unknown error".to_owned(), ToString::to_string),
                ));
            }
        }
    }
    let blocks: Vec<_> = job
        .result()
        .expect("ready block job has output")
        .leaves()
        .map(|leaf| DetachedBlock {
            id: PageId(leaf.id.0),
            start: leaf.physical_start,
            end: leaf.physical_end,
            container_depth: u8::try_from(leaf.context.depth()).expect("bounded container depth"),
        })
        .collect();
    drop(job);

    let mut allocations = AllocationReceipt::default();
    let mut pages = Vec::with_capacity(blocks.len().max(1));
    if blocks.is_empty() {
        pages.push(OutputPage::from_fragment(
            PageId(0),
            &text,
            Vec::new(),
            Vec::new(),
            &mut allocations,
        )?);
    } else {
        for (index, block) in blocks.iter().enumerate() {
            let coverage_start = if index == 0 { 0 } else { block.start };
            let coverage_end = blocks.get(index + 1).map_or(text.len(), |next| next.start);
            if coverage_start > block.start
                || block.start > block.end
                || block.end > coverage_end
                || !text.is_char_boundary(coverage_start)
                || !text.is_char_boundary(block.start)
                || !text.is_char_boundary(block.end)
                || !text.is_char_boundary(coverage_end)
            {
                return Err(DetachError::InvalidBlockRange);
            }
            let fragment = &text[coverage_start..coverage_end];
            let local_start = &text[coverage_start..block.start];
            let local_end = &text[coverage_start..block.end];
            let fact = BlockFact {
                kind: BlockKind::Paragraph,
                range: LocalRange {
                    start: Position::of(local_start),
                    end: Position::of(local_end),
                },
                container_depth: block.container_depth,
                list_property: None,
            };
            pages.push(OutputPage::from_fragment(
                block.id,
                fragment,
                vec![fact],
                Vec::new(),
                &mut allocations,
            )?);
        }
    }
    let tree = OutputTree::from_pages(&pages, &mut allocations);
    let retained_strong_source_leases = Arc::strong_count(&source).saturating_sub(baseline_strong);
    let retained_weak_source_leases = Arc::weak_count(&source).saturating_sub(baseline_weak);
    Ok((
        tree,
        DetachReceipt {
            source_bytes_materialized: text.len(),
            parsed_block_leaves: blocks.len(),
            retained_strong_source_leases,
            retained_weak_source_leases,
            allocations,
        },
    ))
}

/// Compile-time assertion used by the gate: persistent output has no borrowed
/// revision lifetime and can cross a worker boundary.
pub fn assert_detached_output_type() {
    fn require<T: Send + Sync + 'static>() {}
    require::<OutputPage>();
    require::<OutputTree>();
}
