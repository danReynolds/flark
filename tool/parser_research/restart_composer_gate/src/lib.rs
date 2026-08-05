//! Representation-neutral proof for exact restart-state composition.
//!
//! This crate deliberately contains no Markdown parser and no selected green
//! representation. It proves the authority boundary between an exact future
//! control continuation and the semantic-prefix recipe required before an old
//! suffix may be attached.

#![forbid(unsafe_code)]

use std::fmt;
use std::sync::Arc;

macro_rules! id_type {
    ($name:ident, $inner:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub $inner);
    };
}

id_type!(RevisionId, u64);
id_type!(LineageId, u64);
id_type!(PieceId, u64);
id_type!(SourceTailId, u64);
id_type!(BlockId, u64);
id_type!(CapabilityId, u64);
id_type!(SemanticRootId, u64);
id_type!(RootGeneration, u64);
id_type!(GrammarVersion, u16);
id_type!(ProfileId, u16);
id_type!(NormalizedLabelId, u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StableAnchor {
    pub piece: PieceId,
    pub offset: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub piece: PieceId,
    pub start: u32,
    pub end: u32,
    pub utf16_len: u32,
}

impl SourceSpan {
    pub fn new(piece: PieceId, start: u32, end: u32, utf16_len: u32) -> Self {
        assert!(start <= end);
        Self {
            piece,
            start,
            end,
            utf16_len,
        }
    }

    pub fn byte_len(self) -> u64 {
        u64::from(self.end - self.start)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunSummary {
    pub bytes: u64,
    pub utf16: u64,
    pub spans: u64,
}

impl RunSummary {
    fn one(span: SourceSpan) -> Self {
        Self {
            bytes: span.byte_len(),
            utf16: u64::from(span.utf16_len),
            spans: 1,
        }
    }

    fn combine(self, other: Self) -> Self {
        Self {
            bytes: self
                .bytes
                .checked_add(other.bytes)
                .expect("byte summary overflow"),
            utf16: self
                .utf16
                .checked_add(other.utf16)
                .expect("UTF-16 summary overflow"),
            spans: self
                .spans
                .checked_add(other.spans)
                .expect("span summary overflow"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RunNode {
    Leaf {
        span: SourceSpan,
        summary: RunSummary,
    },
    Concat {
        left: Arc<RunNode>,
        right: Arc<RunNode>,
        summary: RunSummary,
    },
}

impl RunNode {
    fn summary(&self) -> RunSummary {
        match self {
            Self::Leaf { summary, .. } | Self::Concat { summary, .. } => *summary,
        }
    }
}

/// A source-backed persistent run expression.
///
/// Concatenation allocates one fixed-size node and retains both inputs. It
/// never copies source bytes and never grows a contiguous string/vector.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceRuns(Option<Arc<RunNode>>);

impl SourceRuns {
    pub fn empty() -> Self {
        Self(None)
    }

    pub fn one(span: SourceSpan) -> Self {
        Self(Some(Arc::new(RunNode::Leaf {
            span,
            summary: RunSummary::one(span),
        })))
    }

    pub fn concat(&self, later: &Self) -> Self {
        match (&self.0, &later.0) {
            (None, _) => later.clone(),
            (_, None) => self.clone(),
            (Some(left), Some(right)) => Self(Some(Arc::new(RunNode::Concat {
                left: Arc::clone(left),
                right: Arc::clone(right),
                summary: left.summary().combine(right.summary()),
            }))),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    pub fn summary(&self) -> RunSummary {
        self.0
            .as_deref()
            .map_or(RunSummary::default(), RunNode::summary)
    }

    /// Exact retained-subtree identity receipt; never used as semantic equality.
    pub fn shares_exact_subtree(&self, other: &Self) -> bool {
        let Some(needle) = other.0.as_ref() else {
            return true;
        };
        let Some(root) = self.0.as_ref() else {
            return false;
        };
        contains_arc(root, needle)
    }

    pub fn contains_piece(&self, piece: PieceId) -> bool {
        self.0
            .as_deref()
            .is_some_and(|node| contains_piece(node, piece))
    }
}

fn contains_arc(node: &Arc<RunNode>, needle: &Arc<RunNode>) -> bool {
    if Arc::ptr_eq(node, needle) {
        return true;
    }
    match node.as_ref() {
        RunNode::Leaf { .. } => false,
        RunNode::Concat { left, right, .. } => {
            contains_arc(left, needle) || contains_arc(right, needle)
        }
    }
}

fn contains_piece(node: &RunNode, piece: PieceId) -> bool {
    match node {
        RunNode::Leaf { span, .. } => span.piece == piece,
        RunNode::Concat { left, right, .. } => {
            contains_piece(left, piece) || contains_piece(right, piece)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Path<T>(Option<Arc<PathNode<T>>>);

impl<T> Default for Path<T> {
    fn default() -> Self {
        Self(None)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PathNode<T> {
    value: T,
    parent: Option<Arc<PathNode<T>>>,
    depth: u32,
}

impl<T> Path<T> {
    fn push(&self, value: T) -> Self {
        let depth = self.0.as_ref().map_or(1, |node| {
            node.depth.checked_add(1).expect("path depth overflow")
        });
        Self(Some(Arc::new(PathNode {
            value,
            parent: self.0.clone(),
            depth,
        })))
    }

    fn head(&self) -> Option<&PathNode<T>> {
        self.0.as_deref()
    }

    fn into_head(self) -> Option<Arc<PathNode<T>>> {
        self.0
    }

    fn depth(&self) -> u32 {
        self.0.as_ref().map_or(0, |node| node.depth)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TrieNode<V> {
    Branch {
        zero: Option<Arc<TrieNode<V>>>,
        one: Option<Arc<TrieNode<V>>>,
    },
    Leaf(V),
}

/// Exact 64-level persistent trie. Label IDs are issued only by the exact
/// normalizer/interner; hashes never authorize a winner lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ExactTrie<V>(Option<Arc<TrieNode<V>>>);

impl<V> Default for ExactTrie<V> {
    fn default() -> Self {
        Self(None)
    }
}

impl<V: Clone> ExactTrie<V> {
    fn get(&self, key: NormalizedLabelId) -> Option<&V> {
        trie_get(self.0.as_deref(), key.0, 0)
    }

    fn insert_first(&self, key: NormalizedLabelId, value: V) -> Self {
        if self.get(key).is_some() {
            return self.clone();
        }
        Self(Some(trie_insert(self.0.as_ref(), key.0, 0, value)))
    }

    fn insert_replace(&self, key: NormalizedLabelId, value: V) -> Self {
        Self(Some(trie_insert(self.0.as_ref(), key.0, 0, value)))
    }
}

fn trie_get<V>(node: Option<&TrieNode<V>>, key: u64, depth: u8) -> Option<&V> {
    let node = node?;
    if depth == 64 {
        return match node {
            TrieNode::Leaf(value) => Some(value),
            TrieNode::Branch { .. } => None,
        };
    }
    match node {
        TrieNode::Leaf(_) => None,
        TrieNode::Branch { zero, one } => {
            let bit = (key >> (63 - depth)) & 1;
            let child = if bit == 0 { zero } else { one };
            trie_get(child.as_deref(), key, depth + 1)
        }
    }
}

fn trie_insert<V: Clone>(
    node: Option<&Arc<TrieNode<V>>>,
    key: u64,
    depth: u8,
    value: V,
) -> Arc<TrieNode<V>> {
    if depth == 64 {
        return Arc::new(TrieNode::Leaf(value));
    }
    let (mut zero, mut one) = match node.map(AsRef::as_ref) {
        Some(TrieNode::Branch { zero, one }) => (zero.clone(), one.clone()),
        Some(TrieNode::Leaf(_)) | None => (None, None),
    };
    let bit = (key >> (63 - depth)) & 1;
    if bit == 0 {
        zero = Some(trie_insert(zero.as_ref(), key, depth + 1, value));
    } else {
        one = Some(trie_insert(one.as_ref(), key, depth + 1, value));
    }
    Arc::new(TrieNode::Branch { zero, one })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListDelimiter {
    Period,
    Parenthesis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawKind {
    FencedCode,
    Html,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlFrame {
    Document,
    OrderedList { delimiter: ListDelimiter },
    Paragraph { table_columns: Option<u16> },
    Raw { kind: RawKind, terminator_class: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlContinuation {
    pub grammar: GrammarVersion,
    pub profile: ProfileId,
    pub at_document_start: bool,
    open: Path<ControlFrame>,
}

impl ControlContinuation {
    pub fn root(grammar: GrammarVersion, profile: ProfileId) -> Self {
        Self {
            grammar,
            profile,
            at_document_start: false,
            open: Path::default(),
        }
    }

    pub fn push(&self, frame: ControlFrame) -> Self {
        let mut next = self.clone();
        next.open = self.open.push(frame);
        next
    }

    pub fn open_depth(&self) -> u32 {
        self.open.depth()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingRole {
    Document,
    List,
    Paragraph,
    Raw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StableBinding {
    pub block: BlockId,
    pub role: BindingRole,
    pub capability: CapabilityId,
    pub opened_at: StableAnchor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StableOpenBindings {
    pub root: SemanticRootId,
    pub root_generation: RootGeneration,
    path: Path<StableBinding>,
}

impl StableOpenBindings {
    pub fn new(root: SemanticRootId, root_generation: RootGeneration) -> Self {
        Self {
            root,
            root_generation,
            path: Path::default(),
        }
    }

    pub fn push(&self, binding: StableBinding) -> Self {
        let mut next = self.clone();
        next.path = self.path.push(binding);
        next
    }

    pub fn open_depth(&self) -> u32 {
        self.path.depth()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChildFold {
    pub direct_children: u64,
    pub contains_blank: bool,
    pub ends_blank: bool,
}

impl ChildFold {
    pub fn one(contains_blank: bool, ends_blank: bool) -> Self {
        Self {
            direct_children: 1,
            contains_blank,
            ends_blank,
        }
    }

    pub fn combine(self, later: Self) -> Self {
        Self {
            direct_children: self
                .direct_children
                .checked_add(later.direct_children)
                .expect("child-count overflow"),
            contains_blank: self.contains_blank || later.contains_blank,
            ends_blank: later.ends_blank,
        }
    }

    pub fn is_tight(self) -> bool {
        !self.contains_blank
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefinitionOccurrence {
    pub label: NormalizedLabelId,
    pub definition_block: BlockId,
    pub source: SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum OccurrenceNode {
    One(DefinitionOccurrence),
    Concat {
        earlier: Arc<OccurrenceNode>,
        later: Arc<OccurrenceNode>,
        count: u64,
    },
}

impl OccurrenceNode {
    fn count(&self) -> u64 {
        match self {
            Self::One(_) => 1,
            Self::Concat { count, .. } => *count,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DefinitionFold {
    occurrences: Option<Arc<OccurrenceNode>>,
    first_winners: ExactTrie<DefinitionOccurrence>,
}

impl DefinitionFold {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn append(&self, occurrence: DefinitionOccurrence) -> Self {
        let one = Arc::new(OccurrenceNode::One(occurrence));
        let occurrences = match &self.occurrences {
            None => one,
            Some(earlier) => Arc::new(OccurrenceNode::Concat {
                count: earlier.count().checked_add(1).expect("definition overflow"),
                earlier: Arc::clone(earlier),
                later: one,
            }),
        };
        Self {
            occurrences: Some(occurrences),
            first_winners: self
                .first_winners
                .insert_first(occurrence.label, occurrence),
        }
    }

    pub fn count(&self) -> u64 {
        self.occurrences.as_deref().map_or(0, OccurrenceNode::count)
    }

    pub fn winner(&self, label: NormalizedLabelId) -> Option<DefinitionOccurrence> {
        self.first_winners.get(label).copied()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangedLabels {
    labels: Path<NormalizedLabelId>,
    membership: ExactTrie<()>,
}

impl ChangedLabels {
    pub fn insert(&self, label: NormalizedLabelId) -> Self {
        if self.membership.get(label).is_some() {
            return self.clone();
        }
        Self {
            labels: self.labels.push(label),
            membership: self.membership.insert_first(label, ()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WinnerIndex(ExactTrie<DefinitionOccurrence>);

impl WinnerIndex {
    pub fn set(&self, occurrence: DefinitionOccurrence) -> Self {
        Self(self.0.insert_replace(occurrence.label, occurrence))
    }

    pub fn get(&self, label: NormalizedLabelId) -> Option<DefinitionOccurrence> {
        self.0.get(label).copied()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsumerSet(Path<BlockId>);

impl ConsumerSet {
    pub fn insert(&self, block: BlockId) -> Self {
        Self(self.0.push(block))
    }

    pub fn len(&self) -> u32 {
        self.0.depth()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsumerIndex(ExactTrie<ConsumerSet>);

impl ConsumerIndex {
    pub fn add(&self, label: NormalizedLabelId, block: BlockId) -> Self {
        let next = self.0.get(label).cloned().unwrap_or_default().insert(block);
        Self(self.0.insert_replace(label, next))
    }

    pub fn get(&self, label: NormalizedLabelId) -> ConsumerSet {
        self.0.get(label).cloned().unwrap_or_default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParagraphHandoff {
    Plain,
    Table {
        preface: SourceRuns,
        header: SourceRuns,
        columns: u16,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListPrefix {
    pub displayed_start: u64,
    pub children: ChildFold,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParagraphPrefix {
    pub visible_runs: SourceRuns,
    pub definitions: DefinitionFold,
    pub changed_labels: ChangedLabels,
    pub handoff: ParagraphHandoff,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawPrefix {
    pub runs: SourceRuns,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticPrefix {
    List(ListPrefix),
    Paragraph(ParagraphPrefix),
    Raw(RawPrefix),
}

impl SemanticPrefix {
    fn role(&self) -> BindingRole {
        match self {
            Self::List(_) => BindingRole::List,
            Self::Paragraph(_) => BindingRole::Paragraph,
            Self::Raw(_) => BindingRole::Raw,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticPrefixFrame {
    pub block: BlockId,
    pub value: SemanticPrefix,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticPrefixState {
    path: Path<SemanticPrefixFrame>,
}

impl SemanticPrefixState {
    pub fn push(&self, frame: SemanticPrefixFrame) -> Self {
        Self {
            path: self.path.push(frame),
        }
    }

    pub fn open_depth(&self) -> u32 {
        self.path.depth()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParagraphTransition {
    End,
    Continue,
    TableDelimiter { columns: u16, table: BlockId },
    SetextUnderline { level: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceSuffix {
    pub definitions: DefinitionFold,
    pub old_global_winners: WinnerIndex,
    pub consumers: ConsumerIndex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParagraphSuffix {
    pub transition: ParagraphTransition,
    pub visible_runs: SourceRuns,
    pub references: ReferenceSuffix,
    pub table_body: ChildFold,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticSuffix {
    List { children: ChildFold },
    Paragraph(ParagraphSuffix),
    Raw { runs: SourceRuns },
}

impl SemanticSuffix {
    fn role(&self) -> BindingRole {
        match self {
            Self::List { .. } => BindingRole::List,
            Self::Paragraph(_) => BindingRole::Paragraph,
            Self::Raw { .. } => BindingRole::Raw,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticSuffixFrame {
    pub block: BlockId,
    pub value: SemanticSuffix,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticSuffixState {
    path: Path<SemanticSuffixFrame>,
}

impl SemanticSuffixState {
    pub fn push(&self, frame: SemanticSuffixFrame) -> Self {
        Self {
            path: self.path.push(frame),
        }
    }

    pub fn open_depth(&self) -> u32 {
        self.path.depth()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceCursor {
    pub revision: RevisionId,
    pub lineage: LineageId,
    pub boundary: StableAnchor,
    pub suffix_tail: SourceTailId,
    pub physical_line: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedulerPhase {
    AtLineBoundary,
    ScanningOversizedLine,
    FinalizingOutput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchedulerCursor {
    pub phase: SchedulerPhase,
    pub work_offset: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestartState {
    pub control: ControlContinuation,
    pub bindings: StableOpenBindings,
    pub semantic_prefix: SemanticPrefixState,
    pub source: SourceCursor,
    pub scheduler: SchedulerCursor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuffixCheckpoint {
    pub control: ControlContinuation,
    pub bindings: StableOpenBindings,
    pub semantic_suffix: SemanticSuffixState,
    pub source: SourceCursor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditLineageProof {
    pub lineage: LineageId,
    pub old_revision: RevisionId,
    pub current_revision: RevisionId,
    pub old_boundary: StableAnchor,
    pub current_boundary: StableAnchor,
    pub unchanged_suffix_tail: SourceTailId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositionContext {
    pub lineage: EditLineageProof,
    pub live_root: SemanticRootId,
    pub live_root_generation: RootGeneration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionRecipe {
    pub earlier: DefinitionFold,
    pub later: DefinitionFold,
}

impl DefinitionRecipe {
    pub fn count(&self) -> u64 {
        self.earlier
            .count()
            .checked_add(self.later.count())
            .expect("definition count overflow")
    }

    pub fn winner(&self, label: NormalizedLabelId) -> Option<DefinitionOccurrence> {
        self.earlier
            .winner(label)
            .or_else(|| self.later.winner(label))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsumerInvalidation {
    pub label: NormalizedLabelId,
    pub old_winner: Option<DefinitionOccurrence>,
    pub new_winner: Option<DefinitionOccurrence>,
    pub consumers: ConsumerSet,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsumerInvalidations(Path<ConsumerInvalidation>);

impl ConsumerInvalidations {
    fn push(&self, invalidation: ConsumerInvalidation) -> Self {
        Self(self.0.push(invalidation))
    }

    pub fn count(&self) -> u32 {
        self.0.depth()
    }

    pub fn find(&self, label: NormalizedLabelId) -> Option<&ConsumerInvalidation> {
        let mut cursor = self.0.head();
        while let Some(node) = cursor {
            if node.value.label == label {
                return Some(&node.value);
            }
            cursor = node.parent.as_deref();
        }
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParagraphDisposition {
    Keep { visible_runs: SourceRuns },
    DetachReferenceOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TablePromotionRecipe {
    WholeParagraph {
        header: SourceRuns,
    },
    SplitPreface {
        preface: SourceRuns,
        header: SourceRuns,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdoptionAction {
    AdoptList {
        block: BlockId,
        displayed_start: u64,
        children: ChildFold,
        tight: bool,
    },
    PromoteTable {
        paragraph: BlockId,
        table: BlockId,
        columns: u16,
        body: ChildFold,
        recipe: TablePromotionRecipe,
    },
    PromoteSetext {
        block: BlockId,
        level: u8,
        content: SourceRuns,
    },
    FinalizeParagraph {
        block: BlockId,
        disposition: ParagraphDisposition,
        definitions: DefinitionRecipe,
        invalidations: ConsumerInvalidations,
    },
    ContinueParagraph {
        block: BlockId,
        runs: SourceRuns,
    },
    SpliceRawRuns {
        block: BlockId,
        runs: SourceRuns,
    },
}

/// One representation-neutral output recipe permanently paired with the exact
/// open binding that authorized it. Storage consumes these in outer-to-inner
/// path order; it never searches the committed tree by [`BlockId`].
#[derive(Debug, PartialEq, Eq)]
pub struct BoundAdoptionAction {
    open_depth: u32,
    binding: StableBinding,
    action: AdoptionAction,
}

impl BoundAdoptionAction {
    #[must_use]
    pub const fn open_depth(&self) -> u32 {
        self.open_depth
    }

    #[must_use]
    pub const fn binding(&self) -> StableBinding {
        self.binding
    }

    #[must_use]
    pub const fn action(&self) -> &AdoptionAction {
        &self.action
    }

    #[must_use]
    pub fn into_action(self) -> AdoptionAction {
        self.action
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct AdoptionActions(Path<BoundAdoptionAction>);

impl AdoptionActions {
    fn push(&self, action: BoundAdoptionAction) -> Self {
        Self(self.0.push(action))
    }

    pub fn count(&self) -> u32 {
        self.0.depth()
    }
}

fn action_block(action: &AdoptionAction) -> BlockId {
    match action {
        AdoptionAction::AdoptList { block, .. }
        | AdoptionAction::PromoteSetext { block, .. }
        | AdoptionAction::FinalizeParagraph { block, .. }
        | AdoptionAction::ContinueParagraph { block, .. }
        | AdoptionAction::SpliceRawRuns { block, .. } => *block,
        AdoptionAction::PromoteTable { paragraph, .. } => *paragraph,
    }
}

fn action_role(action: &AdoptionAction) -> BindingRole {
    match action {
        AdoptionAction::AdoptList { .. } => BindingRole::List,
        AdoptionAction::PromoteTable { .. }
        | AdoptionAction::PromoteSetext { .. }
        | AdoptionAction::FinalizeParagraph { .. }
        | AdoptionAction::ContinueParagraph { .. } => BindingRole::Paragraph,
        AdoptionAction::SpliceRawRuns { .. } => BindingRole::Raw,
    }
}

/// Exact immutable-base identity retained after the composer validates it.
/// This is the storage-facing guard against applying a semantically valid
/// recipe to the wrong root, revision, mapped range, or suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdoptionStamp {
    pub root: SemanticRootId,
    pub root_generation: RootGeneration,
    pub lineage: LineageId,
    pub old_revision: RevisionId,
    pub current_revision: RevisionId,
    pub old_boundary: StableAnchor,
    pub current_boundary: StableAnchor,
    pub suffix_tail: SourceTailId,
}

impl AdoptionStamp {
    fn from_context(context: CompositionContext) -> Self {
        Self {
            root: context.live_root,
            root_generation: context.live_root_generation,
            lineage: context.lineage.lineage,
            old_revision: context.lineage.old_revision,
            current_revision: context.lineage.current_revision,
            old_boundary: context.lineage.old_boundary,
            current_boundary: context.lineage.current_boundary,
            suffix_tail: context.lineage.unchanged_suffix_tail,
        }
    }
}

/// Exact base proof supplied by the selected storage transaction. Bindings are
/// ordered outer-to-inner and carry its already-resolved revision-local
/// capabilities; this is not a document-wide locator table.
#[derive(Clone, Copy, Debug)]
pub struct StorageAdoptionContext<'a> {
    pub stamp: AdoptionStamp,
    pub bindings_outer_to_inner: &'a [StableBinding],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdoptionUseRejection {
    RootMismatch,
    RootGenerationMismatch,
    LineageMismatch,
    RevisionMismatch,
    BoundaryMismatch,
    SuffixIdentityMismatch,
    BindingPathShapeMismatch,
    BindingMismatch { open_depth: u32 },
    ActionOrderMismatch { expected: u32, actual: u32 },
}

/// One-use, storage-authorized plan. Its consuming iterator yields each action
/// together with the exact base capability that locates its Enter/property
/// group.
#[derive(Debug, PartialEq, Eq)]
pub struct StorageAdoptionPlan {
    stamp: AdoptionStamp,
    actions: AdoptionActions,
}

impl StorageAdoptionPlan {
    #[must_use]
    pub const fn stamp(&self) -> AdoptionStamp {
        self.stamp
    }

    #[must_use]
    pub fn into_actions(self) -> BoundAdoptionIntoIter {
        let remaining = self.actions.count();
        BoundAdoptionIntoIter {
            next: self.actions.0.into_head(),
            remaining,
        }
    }
}

pub struct BoundAdoptionIntoIter {
    next: Option<Arc<PathNode<BoundAdoptionAction>>>,
    remaining: u32,
}

impl Iterator for BoundAdoptionIntoIter {
    type Item = BoundAdoptionAction;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next.take()?;
        let node = Arc::try_unwrap(current)
            .expect("one-use adoption action path must have unique ownership");
        self.next = node.parent;
        self.remaining -= 1;
        Some(node.value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = usize::try_from(self.remaining).expect("u32 fits usize");
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for BoundAdoptionIntoIter {}

/// The only type that authorizes suffix attachment. Its fields and constructor
/// are private; a [`ControlWitness`] cannot be converted without semantic
/// composition.
#[derive(Debug, PartialEq, Eq)]
pub struct AdoptionPermit {
    stamp: AdoptionStamp,
    actions: AdoptionActions,
}

impl AdoptionPermit {
    #[must_use]
    pub const fn stamp(&self) -> AdoptionStamp {
        self.stamp
    }

    pub fn suffix_tail(&self) -> SourceTailId {
        self.stamp.suffix_tail
    }

    pub fn current_revision(&self) -> RevisionId {
        self.stamp.current_revision
    }

    pub fn actions(&self) -> &AdoptionActions {
        &self.actions
    }

    /// Validate the exact immutable base and ordered capability path before
    /// transferring this one-use authorization to selected storage.
    ///
    /// ```compile_fail
    /// use restart_composer_gate::{AdoptionPermit, StorageAdoptionContext};
    /// fn replay(permit: AdoptionPermit, context: StorageAdoptionContext<'_>) {
    ///     let _first = permit.authorize_storage(context);
    ///     let _second = permit.authorize_storage(context); // permit was moved
    /// }
    /// ```
    pub fn authorize_storage(
        self,
        context: StorageAdoptionContext<'_>,
    ) -> Result<StorageAdoptionPlan, AdoptionUseRejection> {
        validate_adoption_stamp(self.stamp, context.stamp)?;
        validate_storage_bindings(&self.actions, context.bindings_outer_to_inner)?;
        Ok(StorageAdoptionPlan {
            stamp: self.stamp,
            actions: self.actions,
        })
    }
}

fn validate_adoption_stamp(
    expected: AdoptionStamp,
    actual: AdoptionStamp,
) -> Result<(), AdoptionUseRejection> {
    if actual.root != expected.root {
        return Err(AdoptionUseRejection::RootMismatch);
    }
    if actual.root_generation != expected.root_generation {
        return Err(AdoptionUseRejection::RootGenerationMismatch);
    }
    if actual.lineage != expected.lineage {
        return Err(AdoptionUseRejection::LineageMismatch);
    }
    if actual.old_revision != expected.old_revision
        || actual.current_revision != expected.current_revision
    {
        return Err(AdoptionUseRejection::RevisionMismatch);
    }
    if actual.old_boundary != expected.old_boundary
        || actual.current_boundary != expected.current_boundary
    {
        return Err(AdoptionUseRejection::BoundaryMismatch);
    }
    if actual.suffix_tail != expected.suffix_tail {
        return Err(AdoptionUseRejection::SuffixIdentityMismatch);
    }
    Ok(())
}

fn validate_storage_bindings(
    actions: &AdoptionActions,
    bindings: &[StableBinding],
) -> Result<(), AdoptionUseRejection> {
    if usize::try_from(actions.count()).expect("u32 fits usize") != bindings.len() {
        return Err(AdoptionUseRejection::BindingPathShapeMismatch);
    }
    let mut cursor = actions.0.head();
    for (index, expected_binding) in bindings.iter().copied().enumerate() {
        let Some(node) = cursor else {
            return Err(AdoptionUseRejection::BindingPathShapeMismatch);
        };
        let expected_depth = u32::try_from(index + 1).expect("binding depth fits u32");
        if node.value.open_depth != expected_depth {
            return Err(AdoptionUseRejection::ActionOrderMismatch {
                expected: expected_depth,
                actual: node.value.open_depth,
            });
        }
        if node.value.binding != expected_binding
            || action_block(&node.value.action) != expected_binding.block
            || action_role(&node.value.action) != expected_binding.role
        {
            return Err(AdoptionUseRejection::BindingMismatch {
                open_depth: expected_depth,
            });
        }
        cursor = node.parent.as_deref();
    }
    if cursor.is_some() {
        return Err(AdoptionUseRejection::BindingPathShapeMismatch);
    }
    Ok(())
}

/// Proof only that future block-control branches are equal. This type has no
/// attachment/permit API.
pub struct ControlWitness<'a> {
    current: &'a RestartState,
    suffix: &'a SuffixCheckpoint,
}

impl fmt::Debug for ControlWitness<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ControlWitness")
            .field("control", &self.current.control)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposeRejection {
    ControlMismatch,
    NotPhysicalLineBoundary,
    LineageMismatch,
    RevisionMismatch,
    BoundaryMismatch,
    SuffixIdentityMismatch,
    StaleBindingLease,
    BindingMismatch,
    PathShapeMismatch,
    SemanticBindingMismatch,
    SemanticVariantMismatch,
    TableDecisionIncomplete,
    TableColumnMismatch,
    InvalidSetextLevel,
}

pub struct Composer;

impl Composer {
    /// Exact control equality is intentionally only a necessary first gate.
    pub fn match_control<'a>(
        current: &'a RestartState,
        suffix: &'a SuffixCheckpoint,
    ) -> Result<ControlWitness<'a>, ComposeRejection> {
        if current.control != suffix.control {
            return Err(ComposeRejection::ControlMismatch);
        }
        if current.scheduler.phase != SchedulerPhase::AtLineBoundary {
            return Err(ComposeRejection::NotPhysicalLineBoundary);
        }
        Ok(ControlWitness { current, suffix })
    }

    /// Consumes all remaining authority proofs and creates typed output recipes.
    pub fn compose(
        witness: ControlWitness<'_>,
        context: CompositionContext,
    ) -> Result<AdoptionPermit, ComposeRejection> {
        validate_lineage(witness.current, witness.suffix, context.lineage)?;
        validate_bindings(
            witness.current,
            witness.suffix,
            context.live_root,
            context.live_root_generation,
        )?;

        let mut control = witness.current.control.open.head();
        let mut binding = witness.current.bindings.path.head();
        let mut prefix = witness.current.semantic_prefix.path.head();
        let mut suffix = witness.suffix.semantic_suffix.path.head();
        let mut actions = AdoptionActions::default();
        let mut open_depth = witness.current.control.open_depth();

        loop {
            match (control, binding, prefix, suffix) {
                (None, None, None, None) => break,
                (Some(control_node), Some(binding_node), Some(prefix_node), Some(suffix_node)) => {
                    let stable = binding_node.value;
                    if prefix_node.value.block != stable.block
                        || suffix_node.value.block != stable.block
                        || prefix_node.value.value.role() != stable.role
                        || suffix_node.value.value.role() != stable.role
                    {
                        return Err(ComposeRejection::SemanticBindingMismatch);
                    }
                    let action = compose_frame(
                        &control_node.value,
                        stable,
                        &prefix_node.value.value,
                        &suffix_node.value.value,
                    )?;
                    actions = actions.push(BoundAdoptionAction {
                        open_depth,
                        binding: stable,
                        action,
                    });
                    open_depth -= 1;
                    control = control_node.parent.as_deref();
                    binding = binding_node.parent.as_deref();
                    prefix = prefix_node.parent.as_deref();
                    suffix = suffix_node.parent.as_deref();
                }
                _ => return Err(ComposeRejection::PathShapeMismatch),
            }
        }
        debug_assert_eq!(open_depth, 0);

        Ok(AdoptionPermit {
            stamp: AdoptionStamp::from_context(context),
            actions,
        })
    }
}

fn validate_lineage(
    current: &RestartState,
    suffix: &SuffixCheckpoint,
    proof: EditLineageProof,
) -> Result<(), ComposeRejection> {
    if current.source.lineage != proof.lineage || suffix.source.lineage != proof.lineage {
        return Err(ComposeRejection::LineageMismatch);
    }
    if current.source.revision != proof.current_revision
        || suffix.source.revision != proof.old_revision
        || proof.current_revision <= proof.old_revision
    {
        return Err(ComposeRejection::RevisionMismatch);
    }
    if current.source.boundary != proof.current_boundary
        || suffix.source.boundary != proof.old_boundary
    {
        return Err(ComposeRejection::BoundaryMismatch);
    }
    if current.source.suffix_tail != proof.unchanged_suffix_tail
        || suffix.source.suffix_tail != proof.unchanged_suffix_tail
    {
        return Err(ComposeRejection::SuffixIdentityMismatch);
    }
    Ok(())
}

fn validate_bindings(
    current: &RestartState,
    suffix: &SuffixCheckpoint,
    live_root: SemanticRootId,
    live: RootGeneration,
) -> Result<(), ComposeRejection> {
    if current.bindings.root != live_root
        || suffix.bindings.root != live_root
        || current.bindings.root_generation != live
        || suffix.bindings.root_generation != live
    {
        return Err(ComposeRejection::StaleBindingLease);
    }
    if current.bindings != suffix.bindings {
        return Err(ComposeRejection::BindingMismatch);
    }
    if current.bindings.open_depth() != current.control.open_depth()
        || current.semantic_prefix.open_depth() != current.control.open_depth()
        || suffix.semantic_suffix.open_depth() != current.control.open_depth()
    {
        return Err(ComposeRejection::PathShapeMismatch);
    }
    Ok(())
}

fn compose_frame(
    control: &ControlFrame,
    binding: StableBinding,
    prefix: &SemanticPrefix,
    suffix: &SemanticSuffix,
) -> Result<AdoptionAction, ComposeRejection> {
    match (control, prefix, suffix) {
        (
            ControlFrame::OrderedList { .. },
            SemanticPrefix::List(prefix),
            SemanticSuffix::List { children: suffix },
        ) => {
            let children = prefix.children.combine(*suffix);
            Ok(AdoptionAction::AdoptList {
                block: binding.block,
                displayed_start: prefix.displayed_start,
                children,
                tight: children.is_tight(),
            })
        }
        (
            ControlFrame::Raw { .. },
            SemanticPrefix::Raw(prefix),
            SemanticSuffix::Raw { runs: suffix },
        ) => Ok(AdoptionAction::SpliceRawRuns {
            block: binding.block,
            runs: prefix.runs.concat(suffix),
        }),
        (
            ControlFrame::Paragraph { table_columns },
            SemanticPrefix::Paragraph(prefix),
            SemanticSuffix::Paragraph(suffix),
        ) => compose_paragraph(*table_columns, binding, prefix, suffix),
        _ => Err(ComposeRejection::SemanticVariantMismatch),
    }
}

fn compose_paragraph(
    table_control: Option<u16>,
    binding: StableBinding,
    prefix: &ParagraphPrefix,
    suffix: &ParagraphSuffix,
) -> Result<AdoptionAction, ComposeRejection> {
    match suffix.transition {
        ParagraphTransition::TableDelimiter { columns, table } => {
            let Some(control_columns) = table_control else {
                return Err(ComposeRejection::TableDecisionIncomplete);
            };
            let ParagraphHandoff::Table {
                preface,
                header,
                columns: prefix_columns,
            } = &prefix.handoff
            else {
                return Err(ComposeRejection::TableDecisionIncomplete);
            };
            if columns != control_columns || columns != *prefix_columns {
                return Err(ComposeRejection::TableColumnMismatch);
            }
            let recipe = if preface.is_empty() {
                TablePromotionRecipe::WholeParagraph {
                    header: header.clone(),
                }
            } else {
                TablePromotionRecipe::SplitPreface {
                    preface: preface.clone(),
                    header: header.clone(),
                }
            };
            Ok(AdoptionAction::PromoteTable {
                paragraph: binding.block,
                table,
                columns,
                body: suffix.table_body,
                recipe,
            })
        }
        ParagraphTransition::SetextUnderline { level } => {
            if !(1..=2).contains(&level) {
                return Err(ComposeRejection::InvalidSetextLevel);
            }
            Ok(AdoptionAction::PromoteSetext {
                block: binding.block,
                level,
                content: prefix.visible_runs.concat(&suffix.visible_runs),
            })
        }
        ParagraphTransition::Continue => Ok(AdoptionAction::ContinueParagraph {
            block: binding.block,
            runs: prefix.visible_runs.concat(&suffix.visible_runs),
        }),
        ParagraphTransition::End => {
            let visible = prefix.visible_runs.concat(&suffix.visible_runs);
            let definitions = DefinitionRecipe {
                earlier: prefix.definitions.clone(),
                later: suffix.references.definitions.clone(),
            };
            let mut invalidations = ConsumerInvalidations::default();
            let mut changed = prefix.changed_labels.labels.head();
            while let Some(node) = changed {
                let label = node.value;
                let old_winner = suffix.references.old_global_winners.get(label);
                let new_winner = definitions.winner(label);
                if old_winner != new_winner {
                    invalidations = invalidations.push(ConsumerInvalidation {
                        label,
                        old_winner,
                        new_winner,
                        consumers: suffix.references.consumers.get(label),
                    });
                }
                changed = node.parent.as_deref();
            }
            let disposition = if visible.is_empty() && definitions.count() > 0 {
                ParagraphDisposition::DetachReferenceOnly
            } else {
                ParagraphDisposition::Keep {
                    visible_runs: visible,
                }
            };
            Ok(AdoptionAction::FinalizeParagraph {
                block: binding.block,
                disposition,
                definitions,
                invalidations,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: RevisionId = RevisionId(10);
    const CURRENT: RevisionId = RevisionId(11);
    const LINEAGE: LineageId = LineageId(7);
    const TAIL: SourceTailId = SourceTailId(500);
    const ROOT: SemanticRootId = SemanticRootId(30);
    const ROOT_GENERATION: RootGeneration = RootGeneration(40);
    const BLOCK: BlockId = BlockId(100);

    fn span(piece: u64, len: u32) -> SourceSpan {
        SourceSpan::new(PieceId(piece), 0, len, len)
    }

    fn stable_binding(
        block: BlockId,
        role: BindingRole,
        capability: CapabilityId,
        piece: PieceId,
    ) -> StableBinding {
        StableBinding {
            block,
            role,
            capability,
            opened_at: StableAnchor { piece, offset: 0 },
        }
    }

    fn binding(role: BindingRole) -> StableOpenBindings {
        StableOpenBindings::new(ROOT, ROOT_GENERATION).push(stable_binding(
            BLOCK,
            role,
            CapabilityId(9),
            PieceId(1),
        ))
    }

    fn cursor(revision: RevisionId, physical_line: u64) -> SourceCursor {
        SourceCursor {
            revision,
            lineage: LINEAGE,
            boundary: StableAnchor {
                piece: PieceId(99),
                offset: 3,
            },
            suffix_tail: TAIL,
            physical_line,
        }
    }

    fn context() -> CompositionContext {
        CompositionContext {
            lineage: EditLineageProof {
                lineage: LINEAGE,
                old_revision: OLD,
                current_revision: CURRENT,
                old_boundary: cursor(OLD, 20).boundary,
                current_boundary: cursor(CURRENT, 21).boundary,
                unchanged_suffix_tail: TAIL,
            },
            live_root: ROOT,
            live_root_generation: ROOT_GENERATION,
        }
    }

    fn state(
        control_frame: ControlFrame,
        role: BindingRole,
        prefix: SemanticPrefix,
    ) -> RestartState {
        RestartState {
            control: ControlContinuation::root(GrammarVersion(3), ProfileId(1)).push(control_frame),
            bindings: binding(role),
            semantic_prefix: SemanticPrefixState::default().push(SemanticPrefixFrame {
                block: BLOCK,
                value: prefix,
            }),
            source: cursor(CURRENT, 21),
            scheduler: SchedulerCursor {
                phase: SchedulerPhase::AtLineBoundary,
                work_offset: 73,
            },
        }
    }

    fn suffix(
        control_frame: ControlFrame,
        role: BindingRole,
        value: SemanticSuffix,
    ) -> SuffixCheckpoint {
        SuffixCheckpoint {
            control: ControlContinuation::root(GrammarVersion(3), ProfileId(1)).push(control_frame),
            bindings: binding(role),
            semantic_suffix: SemanticSuffixState::default().push(SemanticSuffixFrame {
                block: BLOCK,
                value,
            }),
            source: cursor(OLD, 20),
        }
    }

    fn empty_references() -> ReferenceSuffix {
        ReferenceSuffix {
            definitions: DefinitionFold::empty(),
            old_global_winners: WinnerIndex::default(),
            consumers: ConsumerIndex::default(),
        }
    }

    fn compose(current: &RestartState, suffix: &SuffixCheckpoint) -> AdoptionPermit {
        let witness = Composer::match_control(current, suffix).expect("control must match");
        Composer::compose(witness, context()).expect("composition must succeed")
    }

    fn action_for_block(permit: &AdoptionPermit, block: BlockId) -> Option<&AdoptionAction> {
        let mut cursor = permit.actions.0.head();
        while let Some(node) = cursor {
            if node.value.binding.block == block {
                return Some(&node.value.action);
            }
            cursor = node.parent.as_deref();
        }
        None
    }

    fn two_frame_states() -> (RestartState, SuffixCheckpoint, [StableBinding; 2]) {
        let outer = stable_binding(
            BlockId(700),
            BindingRole::List,
            CapabilityId(701),
            PieceId(702),
        );
        let inner = stable_binding(
            BlockId(800),
            BindingRole::Raw,
            CapabilityId(801),
            PieceId(802),
        );
        let control = ControlContinuation::root(GrammarVersion(3), ProfileId(1))
            .push(ControlFrame::OrderedList {
                delimiter: ListDelimiter::Period,
            })
            .push(ControlFrame::Raw {
                kind: RawKind::Html,
                terminator_class: 4,
            });
        let bindings = StableOpenBindings::new(ROOT, ROOT_GENERATION)
            .push(outer)
            .push(inner);
        let current = RestartState {
            control: control.clone(),
            bindings: bindings.clone(),
            semantic_prefix: SemanticPrefixState::default()
                .push(SemanticPrefixFrame {
                    block: outer.block,
                    value: SemanticPrefix::List(ListPrefix {
                        displayed_start: 17,
                        children: ChildFold::one(true, false),
                    }),
                })
                .push(SemanticPrefixFrame {
                    block: inner.block,
                    value: SemanticPrefix::Raw(RawPrefix {
                        runs: SourceRuns::one(span(803, 3)),
                    }),
                }),
            source: cursor(CURRENT, 21),
            scheduler: SchedulerCursor {
                phase: SchedulerPhase::AtLineBoundary,
                work_offset: 0,
            },
        };
        let suffix = SuffixCheckpoint {
            control,
            bindings,
            semantic_suffix: SemanticSuffixState::default()
                .push(SemanticSuffixFrame {
                    block: outer.block,
                    value: SemanticSuffix::List {
                        children: ChildFold::one(false, false),
                    },
                })
                .push(SemanticSuffixFrame {
                    block: inner.block,
                    value: SemanticSuffix::Raw {
                        runs: SourceRuns::one(span(804, 5)),
                    },
                }),
            source: cursor(OLD, 20),
        };
        (current, suffix, [outer, inner])
    }

    #[test]
    fn equal_control_cannot_restore_changed_ordered_list_output() {
        let frame = ControlFrame::OrderedList {
            delimiter: ListDelimiter::Period,
        };
        let old_prefix = ListPrefix {
            displayed_start: 1,
            children: ChildFold::one(false, false),
        };
        let new_prefix = ListPrefix {
            displayed_start: 37,
            children: ChildFold::one(true, false),
        };
        let old = state(
            frame.clone(),
            BindingRole::List,
            SemanticPrefix::List(old_prefix),
        );
        let current = state(
            frame.clone(),
            BindingRole::List,
            SemanticPrefix::List(new_prefix),
        );
        let suffix = suffix(
            frame,
            BindingRole::List,
            SemanticSuffix::List {
                children: ChildFold::one(false, false),
            },
        );

        assert_eq!(old.control, current.control);
        let witness = Composer::match_control(&current, &suffix).unwrap();
        assert_eq!(witness.current.control, witness.suffix.control);
        let old_permit = compose(&old, &suffix);
        let permit = Composer::compose(witness, context()).unwrap();
        assert_ne!(old_permit.actions(), permit.actions());
        assert_eq!(
            action_for_block(&permit, BLOCK),
            Some(&AdoptionAction::AdoptList {
                block: BLOCK,
                displayed_start: 37,
                children: ChildFold {
                    direct_children: 2,
                    contains_blank: true,
                    ends_blank: false,
                },
                tight: false,
            })
        );
    }

    #[test]
    fn equal_two_column_control_selects_distinct_table_adoption_recipes() {
        let frame = ControlFrame::Paragraph {
            table_columns: Some(2),
        };
        let header = SourceRuns::one(span(2, 9));
        let no_preface = state(
            frame.clone(),
            BindingRole::Paragraph,
            SemanticPrefix::Paragraph(ParagraphPrefix {
                visible_runs: header.clone(),
                definitions: DefinitionFold::empty(),
                changed_labels: ChangedLabels::default(),
                handoff: ParagraphHandoff::Table {
                    preface: SourceRuns::empty(),
                    header: header.clone(),
                    columns: 2,
                },
            }),
        );
        let preface = SourceRuns::one(span(1, 11));
        let multiline = state(
            frame.clone(),
            BindingRole::Paragraph,
            SemanticPrefix::Paragraph(ParagraphPrefix {
                visible_runs: preface.concat(&header),
                definitions: DefinitionFold::empty(),
                changed_labels: ChangedLabels::default(),
                handoff: ParagraphHandoff::Table {
                    preface: preface.clone(),
                    header: header.clone(),
                    columns: 2,
                },
            }),
        );
        let suffix = suffix(
            frame,
            BindingRole::Paragraph,
            SemanticSuffix::Paragraph(ParagraphSuffix {
                transition: ParagraphTransition::TableDelimiter {
                    columns: 2,
                    table: BlockId(101),
                },
                visible_runs: SourceRuns::empty(),
                references: empty_references(),
                table_body: ChildFold::one(false, false),
            }),
        );

        assert_eq!(no_preface.control, multiline.control);
        let whole = compose(&no_preface, &suffix);
        let split = compose(&multiline, &suffix);
        assert_ne!(whole.actions(), split.actions());
        assert!(matches!(
            action_for_block(&whole, BLOCK),
            Some(AdoptionAction::PromoteTable {
                recipe: TablePromotionRecipe::WholeParagraph { .. },
                ..
            })
        ));
        assert!(matches!(
            action_for_block(&split, BLOCK),
            Some(AdoptionAction::PromoteTable {
                recipe: TablePromotionRecipe::SplitPreface { preface: actual, .. },
                ..
            }) if actual == &preface
        ));
    }

    #[test]
    fn setext_promotion_preserves_binding_and_uses_current_source_runs() {
        let frame = ControlFrame::Paragraph {
            table_columns: None,
        };
        let current_runs = SourceRuns::one(span(44, 12));
        let current = state(
            frame.clone(),
            BindingRole::Paragraph,
            SemanticPrefix::Paragraph(ParagraphPrefix {
                visible_runs: current_runs.clone(),
                definitions: DefinitionFold::empty(),
                changed_labels: ChangedLabels::default(),
                handoff: ParagraphHandoff::Plain,
            }),
        );
        let suffix = suffix(
            frame,
            BindingRole::Paragraph,
            SemanticSuffix::Paragraph(ParagraphSuffix {
                transition: ParagraphTransition::SetextUnderline { level: 2 },
                visible_runs: SourceRuns::empty(),
                references: empty_references(),
                table_body: ChildFold::default(),
            }),
        );
        assert_eq!(
            action_for_block(&compose(&current, &suffix), BLOCK),
            Some(&AdoptionAction::PromoteSetext {
                block: BLOCK,
                level: 2,
                content: current_runs,
            })
        );
    }

    #[test]
    fn reference_only_paragraph_detaches_and_duplicate_first_winner_invalidates_consumers() {
        let label = NormalizedLabelId(8);
        let removed_first = DefinitionOccurrence {
            label,
            definition_block: BlockId(201),
            source: span(60, 8),
        };
        let suffix_duplicate = DefinitionOccurrence {
            label,
            definition_block: BlockId(202),
            source: span(61, 8),
        };
        let current_definition = DefinitionOccurrence {
            label: NormalizedLabelId(9),
            definition_block: BLOCK,
            source: span(62, 8),
        };
        let frame = ControlFrame::Paragraph {
            table_columns: None,
        };
        let current = state(
            frame.clone(),
            BindingRole::Paragraph,
            SemanticPrefix::Paragraph(ParagraphPrefix {
                visible_runs: SourceRuns::empty(),
                definitions: DefinitionFold::empty().append(current_definition),
                changed_labels: ChangedLabels::default().insert(label),
                handoff: ParagraphHandoff::Plain,
            }),
        );
        let refs = ReferenceSuffix {
            definitions: DefinitionFold::empty().append(suffix_duplicate),
            old_global_winners: WinnerIndex::default().set(removed_first),
            consumers: ConsumerIndex::default()
                .add(label, BlockId(301))
                .add(label, BlockId(302)),
        };
        let suffix = suffix(
            frame,
            BindingRole::Paragraph,
            SemanticSuffix::Paragraph(ParagraphSuffix {
                transition: ParagraphTransition::End,
                visible_runs: SourceRuns::empty(),
                references: refs,
                table_body: ChildFold::default(),
            }),
        );

        let permit = compose(&current, &suffix);
        let Some(AdoptionAction::FinalizeParagraph {
            disposition,
            definitions,
            invalidations,
            ..
        }) = action_for_block(&permit, BLOCK)
        else {
            panic!("expected reference finalization")
        };
        assert_eq!(disposition, &ParagraphDisposition::DetachReferenceOnly);
        assert_eq!(definitions.count(), 2);
        assert_eq!(definitions.winner(label), Some(suffix_duplicate));
        let invalidation = invalidations.find(label).unwrap();
        assert_eq!(invalidation.old_winner, Some(removed_first));
        assert_eq!(invalidation.new_winner, Some(suffix_duplicate));
        assert_eq!(invalidation.consumers.len(), 2);
    }

    #[test]
    fn raw_run_splice_retains_exact_suffix_subtree() {
        let frame = ControlFrame::Raw {
            kind: RawKind::FencedCode,
            terminator_class: 3,
        };
        let prefix = SourceRuns::one(span(70, 10));
        let suffix_runs = SourceRuns::one(span(71, 4_096));
        let current = state(
            frame.clone(),
            BindingRole::Raw,
            SemanticPrefix::Raw(RawPrefix { runs: prefix }),
        );
        let suffix = suffix(
            frame,
            BindingRole::Raw,
            SemanticSuffix::Raw {
                runs: suffix_runs.clone(),
            },
        );

        let permit = compose(&current, &suffix);
        let Some(AdoptionAction::SpliceRawRuns { runs, .. }) = action_for_block(&permit, BLOCK)
        else {
            panic!("expected raw splice")
        };
        assert_eq!(runs.summary().bytes, 4_106);
        assert!(runs.contains_piece(PieceId(70)));
        assert!(runs.contains_piece(PieceId(71)));
        assert!(runs.shares_exact_subtree(&suffix_runs));
    }

    #[test]
    fn scheduler_progress_is_disjoint_from_convergence_authority() {
        let frame = ControlFrame::Raw {
            kind: RawKind::Html,
            terminator_class: 2,
        };
        let mut current = state(
            frame.clone(),
            BindingRole::Raw,
            SemanticPrefix::Raw(RawPrefix {
                runs: SourceRuns::one(span(80, 3)),
            }),
        );
        let suffix = suffix(
            frame,
            BindingRole::Raw,
            SemanticSuffix::Raw {
                runs: SourceRuns::one(span(81, 3)),
            },
        );
        let first = compose(&current, &suffix);
        current.scheduler.work_offset = 99_999;
        let second = compose(&current, &suffix);
        assert_eq!(first, second);
    }

    #[test]
    fn mid_line_pause_cannot_be_a_convergence_candidate() {
        let frame = ControlFrame::Raw {
            kind: RawKind::Html,
            terminator_class: 2,
        };
        let mut current = state(
            frame.clone(),
            BindingRole::Raw,
            SemanticPrefix::Raw(RawPrefix {
                runs: SourceRuns::empty(),
            }),
        );
        current.scheduler.phase = SchedulerPhase::ScanningOversizedLine;
        let suffix = suffix(
            frame,
            BindingRole::Raw,
            SemanticSuffix::Raw {
                runs: SourceRuns::empty(),
            },
        );
        assert!(matches!(
            Composer::match_control(&current, &suffix),
            Err(ComposeRejection::NotPhysicalLineBoundary)
        ));
    }

    #[test]
    fn incompatible_and_stale_bindings_are_rejected_after_control_matches() {
        let frame = ControlFrame::Raw {
            kind: RawKind::Html,
            terminator_class: 2,
        };
        let current = state(
            frame.clone(),
            BindingRole::Raw,
            SemanticPrefix::Raw(RawPrefix {
                runs: SourceRuns::empty(),
            }),
        );
        let mut incompatible = suffix(
            frame.clone(),
            BindingRole::Raw,
            SemanticSuffix::Raw {
                runs: SourceRuns::empty(),
            },
        );
        incompatible.bindings =
            StableOpenBindings::new(ROOT, ROOT_GENERATION).push(StableBinding {
                block: BLOCK,
                role: BindingRole::Raw,
                capability: CapabilityId(999),
                opened_at: StableAnchor {
                    piece: PieceId(1),
                    offset: 0,
                },
            });
        let witness = Composer::match_control(&current, &incompatible).unwrap();
        assert_eq!(
            Composer::compose(witness, context()),
            Err(ComposeRejection::BindingMismatch)
        );

        let compatible = suffix(
            frame,
            BindingRole::Raw,
            SemanticSuffix::Raw {
                runs: SourceRuns::empty(),
            },
        );
        let witness = Composer::match_control(&current, &compatible).unwrap();
        let mut stale_context = context();
        stale_context.live_root_generation = RootGeneration(ROOT_GENERATION.0 + 1);
        assert_eq!(
            Composer::compose(witness, stale_context),
            Err(ComposeRejection::StaleBindingLease)
        );
    }

    #[test]
    fn revision_lineage_boundary_and_suffix_identity_are_each_authoritative() {
        let frame = ControlFrame::Raw {
            kind: RawKind::Html,
            terminator_class: 2,
        };
        let current = state(
            frame.clone(),
            BindingRole::Raw,
            SemanticPrefix::Raw(RawPrefix {
                runs: SourceRuns::empty(),
            }),
        );
        let suffix = suffix(
            frame,
            BindingRole::Raw,
            SemanticSuffix::Raw {
                runs: SourceRuns::empty(),
            },
        );

        let mut bad = context();
        bad.lineage.lineage = LineageId(999);
        assert_eq!(
            Composer::compose(Composer::match_control(&current, &suffix).unwrap(), bad),
            Err(ComposeRejection::LineageMismatch)
        );

        let mut bad = context();
        bad.lineage.current_revision = RevisionId(12);
        assert_eq!(
            Composer::compose(Composer::match_control(&current, &suffix).unwrap(), bad),
            Err(ComposeRejection::RevisionMismatch)
        );

        let mut bad = context();
        bad.lineage.current_boundary.offset += 1;
        assert_eq!(
            Composer::compose(Composer::match_control(&current, &suffix).unwrap(), bad),
            Err(ComposeRejection::BoundaryMismatch)
        );

        let mut bad = context();
        bad.lineage.unchanged_suffix_tail = SourceTailId(999);
        assert_eq!(
            Composer::compose(Composer::match_control(&current, &suffix).unwrap(), bad),
            Err(ComposeRejection::SuffixIdentityMismatch)
        );
    }

    #[test]
    fn changed_paragraph_prefix_cannot_restore_old_visible_output() {
        let frame = ControlFrame::Paragraph {
            table_columns: None,
        };
        let old = state(
            frame.clone(),
            BindingRole::Paragraph,
            SemanticPrefix::Paragraph(ParagraphPrefix {
                visible_runs: SourceRuns::one(span(90, 5)),
                definitions: DefinitionFold::empty(),
                changed_labels: ChangedLabels::default(),
                handoff: ParagraphHandoff::Plain,
            }),
        );
        let current_runs = SourceRuns::one(span(91, 7));
        let current = state(
            frame.clone(),
            BindingRole::Paragraph,
            SemanticPrefix::Paragraph(ParagraphPrefix {
                visible_runs: current_runs.clone(),
                definitions: DefinitionFold::empty(),
                changed_labels: ChangedLabels::default(),
                handoff: ParagraphHandoff::Plain,
            }),
        );
        let suffix_runs = SourceRuns::one(span(92, 5));
        let suffix = suffix(
            frame,
            BindingRole::Paragraph,
            SemanticSuffix::Paragraph(ParagraphSuffix {
                transition: ParagraphTransition::Continue,
                visible_runs: suffix_runs.clone(),
                references: empty_references(),
                table_body: ChildFold::default(),
            }),
        );

        assert_eq!(old.control, current.control);
        let old_output = compose(&old, &suffix);
        let current_output = compose(&current, &suffix);
        assert_ne!(old_output.actions(), current_output.actions());
        let Some(AdoptionAction::ContinueParagraph { runs, .. }) =
            action_for_block(&current_output, BLOCK)
        else {
            panic!("expected paragraph continuation")
        };
        assert!(runs.contains_piece(PieceId(91)));
        assert!(!runs.contains_piece(PieceId(90)));
        assert!(runs.shares_exact_subtree(&current_runs));
        assert!(runs.shares_exact_subtree(&suffix_runs));
    }

    #[test]
    fn transition_variant_or_incomplete_table_state_cannot_attach() {
        let control = ControlFrame::Paragraph {
            table_columns: None,
        };
        let current = state(
            control.clone(),
            BindingRole::Paragraph,
            SemanticPrefix::Paragraph(ParagraphPrefix {
                visible_runs: SourceRuns::one(span(100, 3)),
                definitions: DefinitionFold::empty(),
                changed_labels: ChangedLabels::default(),
                handoff: ParagraphHandoff::Plain,
            }),
        );
        let suffix = suffix(
            control,
            BindingRole::Paragraph,
            SemanticSuffix::Paragraph(ParagraphSuffix {
                transition: ParagraphTransition::TableDelimiter {
                    columns: 2,
                    table: BlockId(900),
                },
                visible_runs: SourceRuns::empty(),
                references: empty_references(),
                table_body: ChildFold::default(),
            }),
        );
        assert_eq!(
            Composer::compose(
                Composer::match_control(&current, &suffix).unwrap(),
                context()
            ),
            Err(ComposeRejection::TableDecisionIncomplete)
        );
    }

    #[test]
    fn storage_consumes_capability_bound_actions_in_outer_to_inner_order() {
        let (current, suffix, bindings) = two_frame_states();
        let permit = compose(&current, &suffix);
        let stamp = permit.stamp();
        let plan = permit
            .authorize_storage(StorageAdoptionContext {
                stamp,
                bindings_outer_to_inner: &bindings,
            })
            .unwrap();
        assert_eq!(plan.stamp(), stamp);

        let mut actions = plan.into_actions();
        assert_eq!(actions.len(), 2);
        let outer = actions.next().unwrap();
        assert_eq!(outer.open_depth(), 1);
        assert_eq!(outer.binding(), bindings[0]);
        assert!(matches!(outer.action(), AdoptionAction::AdoptList { .. }));
        let inner = actions.next().unwrap();
        assert_eq!(inner.open_depth(), 2);
        assert_eq!(inner.binding(), bindings[1]);
        assert!(matches!(
            inner.action(),
            AdoptionAction::SpliceRawRuns { .. }
        ));
        assert!(actions.next().is_none());
    }

    #[test]
    fn storage_rejects_every_base_stamp_substitution_before_yielding_actions() {
        let frame = ControlFrame::Raw {
            kind: RawKind::Html,
            terminator_class: 2,
        };
        let current = state(
            frame.clone(),
            BindingRole::Raw,
            SemanticPrefix::Raw(RawPrefix {
                runs: SourceRuns::empty(),
            }),
        );
        let suffix = suffix(
            frame,
            BindingRole::Raw,
            SemanticSuffix::Raw {
                runs: SourceRuns::empty(),
            },
        );
        let bindings = [stable_binding(
            BLOCK,
            BindingRole::Raw,
            CapabilityId(9),
            PieceId(1),
        )];
        let expected = compose(&current, &suffix).stamp();
        let reject = |stamp| {
            compose(&current, &suffix)
                .authorize_storage(StorageAdoptionContext {
                    stamp,
                    bindings_outer_to_inner: &bindings,
                })
                .unwrap_err()
        };

        assert_eq!(
            reject(AdoptionStamp {
                root: SemanticRootId(expected.root.0 + 1),
                ..expected
            }),
            AdoptionUseRejection::RootMismatch
        );
        assert_eq!(
            reject(AdoptionStamp {
                root_generation: RootGeneration(expected.root_generation.0 + 1),
                ..expected
            }),
            AdoptionUseRejection::RootGenerationMismatch
        );
        assert_eq!(
            reject(AdoptionStamp {
                lineage: LineageId(expected.lineage.0 + 1),
                ..expected
            }),
            AdoptionUseRejection::LineageMismatch
        );
        assert_eq!(
            reject(AdoptionStamp {
                old_revision: RevisionId(expected.old_revision.0 - 1),
                ..expected
            }),
            AdoptionUseRejection::RevisionMismatch
        );
        assert_eq!(
            reject(AdoptionStamp {
                current_revision: RevisionId(expected.current_revision.0 + 1),
                ..expected
            }),
            AdoptionUseRejection::RevisionMismatch
        );
        assert_eq!(
            reject(AdoptionStamp {
                old_boundary: StableAnchor {
                    offset: expected.old_boundary.offset + 1,
                    ..expected.old_boundary
                },
                ..expected
            }),
            AdoptionUseRejection::BoundaryMismatch
        );
        assert_eq!(
            reject(AdoptionStamp {
                current_boundary: StableAnchor {
                    offset: expected.current_boundary.offset + 1,
                    ..expected.current_boundary
                },
                ..expected
            }),
            AdoptionUseRejection::BoundaryMismatch
        );
        assert_eq!(
            reject(AdoptionStamp {
                suffix_tail: SourceTailId(expected.suffix_tail.0 + 1),
                ..expected
            }),
            AdoptionUseRejection::SuffixIdentityMismatch
        );
    }

    #[test]
    fn storage_rejects_missing_wrong_or_reordered_capability_paths() {
        let (current, suffix, bindings) = two_frame_states();
        let expected = compose(&current, &suffix).stamp();
        let authorize = |candidate: &[StableBinding]| {
            compose(&current, &suffix)
                .authorize_storage(StorageAdoptionContext {
                    stamp: expected,
                    bindings_outer_to_inner: candidate,
                })
                .unwrap_err()
        };

        assert_eq!(
            authorize(&bindings[..1]),
            AdoptionUseRejection::BindingPathShapeMismatch
        );
        let mut wrong_capability = bindings;
        wrong_capability[1].capability = CapabilityId(bindings[1].capability.0 + 1);
        assert_eq!(
            authorize(&wrong_capability),
            AdoptionUseRejection::BindingMismatch { open_depth: 2 }
        );
        assert_eq!(
            authorize(&[bindings[1], bindings[0]]),
            AdoptionUseRejection::BindingMismatch { open_depth: 1 }
        );
    }
}
