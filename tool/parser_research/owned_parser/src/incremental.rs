use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::block_machine::{
    advance_state, leading_spaces, strip_quote_prefixes, BlockState, LeafState,
};
use crate::SourceRope;

#[derive(Clone, Debug, PartialEq, Eq)]
struct LineRecord {
    len: usize,
    hash: u64,
    state_after: BlockState,
    order_key: u128,
    facts: LineFacts,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LineFacts {
    definition: Option<DefinitionFact>,
    lookups: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DefinitionFact {
    label: String,
    destination: String,
    title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ReferenceIndex {
    definitions: HashMap<String, BTreeMap<u128, (String, String)>>,
    lookups: HashMap<String, BTreeSet<u128>>,
}

impl ReferenceIndex {
    fn from_tree(tree: &CheckpointTree) -> Self {
        let mut index = Self::default();
        index.add_records(&tree.records_in_range(0, tree.records()));
        index
    }

    fn winner(&self, label: &str) -> Option<&(String, String)> {
        self.definitions
            .get(label)
            .and_then(|occurrences| occurrences.first_key_value().map(|(_, value)| value))
    }

    fn semantic_snapshot(&self) -> BTreeMap<String, (Vec<(String, String)>, usize)> {
        let labels = self
            .definitions
            .keys()
            .chain(self.lookups.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        labels
            .into_iter()
            .map(|label| {
                let definitions = self
                    .definitions
                    .get(&label)
                    .map_or_else(Vec::new, |values| values.values().cloned().collect());
                let lookups = self.lookups.get(&label).map_or(0, BTreeSet::len);
                (label, (definitions, lookups))
            })
            .collect()
    }

    fn remove_records(&mut self, records: &[Arc<LineRecord>]) {
        for record in records {
            if let Some(definition) = &record.facts.definition {
                if let Some(occurrences) = self.definitions.get_mut(&definition.label) {
                    occurrences.remove(&record.order_key);
                    if occurrences.is_empty() {
                        self.definitions.remove(&definition.label);
                    }
                }
            }
            for label in &record.facts.lookups {
                if let Some(lookups) = self.lookups.get_mut(label) {
                    lookups.remove(&record.order_key);
                    if lookups.is_empty() {
                        self.lookups.remove(label);
                    }
                }
            }
        }
    }

    fn add_records(&mut self, records: &[Arc<LineRecord>]) {
        for record in records {
            if let Some(definition) = &record.facts.definition {
                self.definitions
                    .entry(definition.label.clone())
                    .or_default()
                    .insert(
                        record.order_key,
                        (definition.destination.clone(), definition.title.clone()),
                    );
            }
            for label in &record.facts.lookups {
                self.lookups
                    .entry(label.clone())
                    .or_default()
                    .insert(record.order_key);
            }
        }
    }

    fn apply(
        &mut self,
        removed: &[Arc<LineRecord>],
        inserted: &[Arc<LineRecord>],
    ) -> ReferenceDelta {
        let impacted = removed
            .iter()
            .chain(inserted)
            .filter_map(|record| record.facts.definition.as_ref())
            .map(|definition| definition.label.clone())
            .collect::<BTreeSet<_>>();
        let before = impacted
            .iter()
            .map(|label| (label.clone(), self.winner(label).cloned()))
            .collect::<HashMap<_, _>>();
        self.remove_records(removed);
        self.add_records(inserted);

        let mut presence_changed = Vec::new();
        let mut value_changed = Vec::new();
        let mut invalidated_lookup_records = 0;
        for label in impacted {
            let old = before[&label].as_ref();
            let new = self.winner(&label);
            if old.is_some() != new.is_some() {
                presence_changed.push(label.clone());
            } else if old != new {
                value_changed.push(label.clone());
            } else {
                continue;
            }
            invalidated_lookup_records += self.lookups.get(&label).map_or(0, BTreeSet::len);
        }
        ReferenceDelta {
            presence_changed,
            value_changed,
            invalidated_lookup_records,
        }
    }
}

#[derive(Clone, Default)]
struct CheckpointTree {
    root: Option<Arc<CheckpointNode>>,
}

#[derive(Debug)]
enum CheckpointNode {
    Leaf(Arc<LineRecord>),
    Branch {
        left: Arc<CheckpointNode>,
        right: Arc<CheckpointNode>,
        bytes: usize,
        records: usize,
        height: usize,
    },
}

impl CheckpointNode {
    fn bytes(&self) -> usize {
        match self {
            Self::Leaf(record) => record.len,
            Self::Branch { bytes, .. } => *bytes,
        }
    }

    fn records(&self) -> usize {
        match self {
            Self::Leaf(_) => 1,
            Self::Branch { records, .. } => *records,
        }
    }

    fn height(&self) -> usize {
        match self {
            Self::Leaf(_) => 1,
            Self::Branch { height, .. } => *height,
        }
    }
}

impl CheckpointTree {
    fn from_records(records: Vec<Arc<LineRecord>>) -> Self {
        fn build(records: &[Arc<LineRecord>]) -> Option<Arc<CheckpointNode>> {
            match records.len() {
                0 => None,
                1 => Some(Arc::new(CheckpointNode::Leaf(records[0].clone()))),
                len => {
                    let middle = len / 2;
                    Some(checkpoint_branch(
                        build(&records[..middle]).unwrap(),
                        build(&records[middle..]).unwrap(),
                    ))
                }
            }
        }
        Self {
            root: build(&records),
        }
    }

    fn from_source(source: &SourceRope) -> Self {
        let mut state = BlockState::default();
        let mut offset = 0;
        let mut parsed = Vec::new();
        while offset < source.len() {
            let (end, line) = source.line_from(offset);
            let facts = scan_line_facts(&line, &state);
            state = advance_state(state, &line);
            parsed.push((end - offset, hash64(line.as_bytes()), state.clone(), facts));
            offset = end;
        }
        let spacing = u128::MAX / (parsed.len() as u128 + 1);
        let records = parsed
            .into_iter()
            .enumerate()
            .map(|(index, (len, hash, state_after, facts))| {
                Arc::new(LineRecord {
                    len,
                    hash,
                    state_after,
                    order_key: spacing * (index as u128 + 1),
                    facts,
                })
            })
            .collect();
        Self::from_records(records)
    }

    fn bytes(&self) -> usize {
        self.root.as_deref().map_or(0, CheckpointNode::bytes)
    }

    fn records(&self) -> usize {
        self.root.as_deref().map_or(0, CheckpointNode::records)
    }

    fn height(&self) -> usize {
        self.root.as_deref().map_or(0, CheckpointNode::height)
    }

    fn record_at(&self, index: usize) -> Arc<LineRecord> {
        assert!(index < self.records());
        let mut node = self.root.as_ref().unwrap().clone();
        let mut remaining = index;
        loop {
            match node.as_ref() {
                CheckpointNode::Leaf(record) => return record.clone(),
                CheckpointNode::Branch { left, right, .. } => {
                    if remaining < left.records() {
                        node = left.clone();
                    } else {
                        remaining -= left.records();
                        node = right.clone();
                    }
                }
            }
        }
    }

    fn records_in_range(&self, start: usize, end: usize) -> Vec<Arc<LineRecord>> {
        assert!(start <= end && end <= self.records());
        (start..end).map(|index| self.record_at(index)).collect()
    }

    fn prefix_bytes(&self, count: usize) -> usize {
        assert!(count <= self.records());
        fn sum(node: &Arc<CheckpointNode>, count: usize) -> usize {
            if count == 0 {
                return 0;
            }
            if count == node.records() {
                return node.bytes();
            }
            match node.as_ref() {
                CheckpointNode::Leaf(_) => unreachable!(),
                CheckpointNode::Branch { left, right, .. } => {
                    if count <= left.records() {
                        sum(left, count)
                    } else {
                        left.bytes() + sum(right, count - left.records())
                    }
                }
            }
        }
        self.root.as_ref().map_or(0, |root| sum(root, count))
    }

    /// Number of complete records whose end is at or before `offset`.
    fn records_before_or_at(&self, offset: usize) -> usize {
        assert!(offset <= self.bytes());
        fn locate(node: &Arc<CheckpointNode>, offset: usize) -> usize {
            if offset == 0 {
                return 0;
            }
            if offset >= node.bytes() {
                return node.records();
            }
            match node.as_ref() {
                CheckpointNode::Leaf(_) => 0,
                CheckpointNode::Branch { left, right, .. } => {
                    if offset < left.bytes() {
                        locate(left, offset)
                    } else {
                        left.records() + locate(right, offset - left.bytes())
                    }
                }
            }
        }
        self.root.as_ref().map_or(0, |root| locate(root, offset))
    }

    fn index_at_boundary(&self, offset: usize) -> Option<usize> {
        let index = self.records_before_or_at(offset);
        (self.prefix_bytes(index) == offset).then_some(index)
    }

    fn replace(&self, start: usize, end: usize, inserted: Self) -> Self {
        assert!(start <= end && end <= self.records());
        let (prefix, rest) = split_checkpoint_tree(self.root.clone(), start);
        let (_, suffix) = split_checkpoint_tree(rest, end - start);
        Self {
            root: concat_checkpoint_tree(concat_checkpoint_tree(prefix, inserted.root), suffix),
        }
    }

    fn signatures(&self) -> Vec<(usize, u64, BlockState, LineFacts)> {
        fn walk(
            node: &Option<Arc<CheckpointNode>>,
            output: &mut Vec<(usize, u64, BlockState, LineFacts)>,
        ) {
            let Some(node) = node else { return };
            match node.as_ref() {
                CheckpointNode::Leaf(record) => output.push((
                    record.len,
                    record.hash,
                    record.state_after.clone(),
                    record.facts.clone(),
                )),
                CheckpointNode::Branch { left, right, .. } => {
                    walk(&Some(left.clone()), output);
                    walk(&Some(right.clone()), output);
                }
            }
        }
        let mut output = Vec::with_capacity(self.records());
        walk(&self.root, &mut output);
        output
    }
}

fn checkpoint_branch(left: Arc<CheckpointNode>, right: Arc<CheckpointNode>) -> Arc<CheckpointNode> {
    Arc::new(CheckpointNode::Branch {
        bytes: left.bytes() + right.bytes(),
        records: left.records() + right.records(),
        height: left.height().max(right.height()) + 1,
        left,
        right,
    })
}

fn split_checkpoint_tree(
    node: Option<Arc<CheckpointNode>>,
    index: usize,
) -> (Option<Arc<CheckpointNode>>, Option<Arc<CheckpointNode>>) {
    let Some(node) = node else {
        assert_eq!(index, 0);
        return (None, None);
    };
    assert!(index <= node.records());
    if index == 0 {
        return (None, Some(node));
    }
    if index == node.records() {
        return (Some(node), None);
    }
    match node.as_ref() {
        CheckpointNode::Leaf(_) => unreachable!(),
        CheckpointNode::Branch { left, right, .. } => {
            if index < left.records() {
                let (prefix, rest) = split_checkpoint_tree(Some(left.clone()), index);
                (prefix, concat_checkpoint_tree(rest, Some(right.clone())))
            } else if index == left.records() {
                (Some(left.clone()), Some(right.clone()))
            } else {
                let (rest, suffix) =
                    split_checkpoint_tree(Some(right.clone()), index - left.records());
                (concat_checkpoint_tree(Some(left.clone()), rest), suffix)
            }
        }
    }
}

fn concat_checkpoint_tree(
    left: Option<Arc<CheckpointNode>>,
    right: Option<Arc<CheckpointNode>>,
) -> Option<Arc<CheckpointNode>> {
    let (left, right) = match (left, right) {
        (None, right) => return right,
        (left, None) => return left,
        (Some(left), Some(right)) => (left, right),
    };
    if left.height() > right.height() + 1 {
        let CheckpointNode::Branch {
            left: outer_left,
            right: inner_left,
            ..
        } = left.as_ref()
        else {
            unreachable!()
        };
        return Some(balance_checkpoint_tree(checkpoint_branch(
            outer_left.clone(),
            concat_checkpoint_tree(Some(inner_left.clone()), Some(right)).unwrap(),
        )));
    }
    if right.height() > left.height() + 1 {
        let CheckpointNode::Branch {
            left: inner_right,
            right: outer_right,
            ..
        } = right.as_ref()
        else {
            unreachable!()
        };
        return Some(balance_checkpoint_tree(checkpoint_branch(
            concat_checkpoint_tree(Some(left), Some(inner_right.clone())).unwrap(),
            outer_right.clone(),
        )));
    }
    Some(checkpoint_branch(left, right))
}

fn balance_checkpoint_tree(node: Arc<CheckpointNode>) -> Arc<CheckpointNode> {
    let CheckpointNode::Branch { left, right, .. } = node.as_ref() else {
        return node;
    };
    let balance = left.height() as isize - right.height() as isize;
    if balance > 1 {
        let CheckpointNode::Branch {
            left: left_left,
            right: left_right,
            ..
        } = left.as_ref()
        else {
            unreachable!()
        };
        if left_left.height() < left_right.height() {
            let CheckpointNode::Branch {
                left: pivot_left,
                right: pivot_right,
                ..
            } = left_right.as_ref()
            else {
                unreachable!()
            };
            return checkpoint_branch(
                checkpoint_branch(left_left.clone(), pivot_left.clone()),
                checkpoint_branch(pivot_right.clone(), right.clone()),
            );
        }
        return checkpoint_branch(
            left_left.clone(),
            checkpoint_branch(left_right.clone(), right.clone()),
        );
    }
    if balance < -1 {
        let CheckpointNode::Branch {
            left: right_left,
            right: right_right,
            ..
        } = right.as_ref()
        else {
            unreachable!()
        };
        if right_right.height() < right_left.height() {
            let CheckpointNode::Branch {
                left: pivot_left,
                right: pivot_right,
                ..
            } = right_left.as_ref()
            else {
                unreachable!()
            };
            return checkpoint_branch(
                checkpoint_branch(left.clone(), pivot_left.clone()),
                checkpoint_branch(pivot_right.clone(), right_right.clone()),
            );
        }
        return checkpoint_branch(
            checkpoint_branch(left.clone(), right_left.clone()),
            right_right.clone(),
        );
    }
    node
}

/// Extract the deliberately narrow reference-definition/dependency slice.
///
/// This is not a complete CommonMark bracket parser. Its purpose is to test
/// whether source-backed persistent records can carry global symbol facts
/// without making ordinary edits document-sized.
fn scan_line_facts(line_with_ending: &str, state_before: &BlockState) -> LineFacts {
    if matches!(
        state_before.leaf,
        LeafState::Fence { .. } | LeafState::HtmlComment | LeafState::IndentedCode
    ) {
        return LineFacts::default();
    }
    let line = line_with_ending
        .strip_suffix('\n')
        .unwrap_or(line_with_ending);
    let (_, inner) = strip_quote_prefixes(line);
    if leading_spaces(inner) >= 4 {
        return LineFacts::default();
    }
    let content = inner.trim_start_matches([' ', '\t']);
    if let Some(definition) = parse_definition_fact(content) {
        return LineFacts {
            definition: Some(definition),
            lookups: Vec::new(),
        };
    }

    let mut lookups = Vec::new();
    let bytes = content.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'[' || (cursor > 0 && bytes[cursor - 1] == b'\\') {
            cursor += 1;
            continue;
        }
        let Some(close) = content[cursor + 1..].find(']').map(|at| cursor + 1 + at) else {
            break;
        };
        let text = &content[cursor + 1..close];
        let (label, end) = if bytes.get(close + 1) == Some(&b'[') {
            let Some(label_close) = content[close + 2..].find(']').map(|at| close + 2 + at) else {
                cursor = close + 1;
                continue;
            };
            (&content[close + 2..label_close], label_close + 1)
        } else {
            (text, close + 1)
        };
        let normalized = normalize_label(label);
        if !normalized.is_empty() && !lookups.contains(&normalized) {
            lookups.push(normalized);
        }
        cursor = end;
    }
    LineFacts {
        definition: None,
        lookups,
    }
}

fn parse_definition_fact(content: &str) -> Option<DefinitionFact> {
    let label_end = content.find("]: ").or_else(|| content.find("]:"))?;
    if !content.starts_with('[') || label_end < 2 {
        return None;
    }
    let label = normalize_label(&content[1..label_end]);
    if label.is_empty() {
        return None;
    }
    let rest = content[label_end + 2..].trim_start();
    let (destination, rest) = if let Some(rest) = rest.strip_prefix('<') {
        let end = rest.find('>')?;
        (rest[..end].to_owned(), rest[end + 1..].trim_start())
    } else {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if end == 0 {
            return None;
        }
        (rest[..end].to_owned(), rest[end..].trim_start())
    };
    let title = if rest.len() >= 2 {
        let first = rest.as_bytes()[0];
        let last = *rest.as_bytes().last().unwrap();
        if matches!((first, last), (b'\'', b'\'') | (b'"', b'"') | (b'(', b')')) {
            rest[1..rest.len() - 1].to_owned()
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    Some(DefinitionFact {
        label,
        destination,
        title,
    })
}

fn normalize_label(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[derive(Clone, Copy, Debug)]
pub struct WorkBudget {
    pub max_lines: usize,
    pub max_bytes: usize,
}

impl WorkBudget {
    pub const fn new(max_lines: usize, max_bytes: usize) -> Self {
        Self {
            max_lines,
            max_bytes,
        }
    }
}

#[derive(Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug)]
pub struct EditRequest {
    pub base_revision: u64,
    pub before_hash32: u32,
    pub start_utf8: usize,
    pub end_utf8: usize,
    pub replacement: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncrementalError {
    Revision,
    Hash,
    Range,
    Utf8Boundary,
    LineEnding,
    NotConverged,
}

#[derive(Clone)]
pub struct RevisionedDocument {
    revision: u64,
    source: SourceRope,
    checkpoints: CheckpointTree,
    references: ReferenceIndex,
}

impl RevisionedDocument {
    pub fn new(source: &str) -> Self {
        let source = SourceRope::from_str(source);
        let checkpoints = CheckpointTree::from_source(&source);
        let references = ReferenceIndex::from_tree(&checkpoints);
        Self {
            revision: 0,
            source,
            checkpoints,
            references,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn hash32(&self) -> u32 {
        self.source.hash32()
    }

    pub fn len_utf8(&self) -> usize {
        self.source.len()
    }

    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.records()
    }

    pub fn checkpoint_height(&self) -> usize {
        self.checkpoints.height()
    }

    pub fn materialize(&self) -> String {
        self.source.materialize()
    }

    pub fn reference_target(&self, label: &str) -> Option<(&str, &str)> {
        self.references
            .winner(&normalize_label(label))
            .map(|(destination, title)| (destination.as_str(), title.as_str()))
    }

    pub fn reference_lookup_count(&self, label: &str) -> usize {
        self.references
            .lookups
            .get(&normalize_label(label))
            .map_or(0, BTreeSet::len)
    }

    pub fn begin_edit(&self, request: EditRequest) -> Result<EditSession, IncrementalError> {
        if request.base_revision != self.revision {
            return Err(IncrementalError::Revision);
        }
        if request.before_hash32 != self.hash32() {
            return Err(IncrementalError::Hash);
        }
        if request.start_utf8 > request.end_utf8 || request.end_utf8 > self.source.len() {
            return Err(IncrementalError::Range);
        }
        if !self.source.is_char_boundary(request.start_utf8)
            || !self.source.is_char_boundary(request.end_utf8)
        {
            return Err(IncrementalError::Utf8Boundary);
        }
        if request.replacement.contains('\r') {
            return Err(IncrementalError::LineEnding);
        }

        let affected = self
            .checkpoints
            .records_before_or_at(request.start_utf8)
            .min(self.checkpoints.records());
        let restart_index = affected.saturating_sub(1);
        let restart_offset = self.checkpoints.prefix_bytes(restart_index);
        let state = restart_index
            .checked_sub(1)
            .map_or_else(BlockState::default, |index| {
                self.checkpoints.record_at(index).state_after.clone()
            });
        let edited =
            self.source
                .replace(request.start_utf8, request.end_utf8, &request.replacement);
        let new_edit_end = request.start_utf8 + request.replacement.len();
        let byte_delta =
            request.replacement.len() as isize - (request.end_utf8 - request.start_utf8) as isize;

        Ok(EditSession {
            base_revision: self.revision,
            before_hash32: self.hash32(),
            old_edit_end: request.end_utf8,
            new_edit_end,
            byte_delta,
            old_checkpoints: self.checkpoints.clone(),
            references: self.references.clone(),
            edited,
            restart_index,
            restart_offset,
            offset: restart_offset,
            state,
            inserted: Vec::new(),
            reparsed_lines: 0,
            reparsed_bytes: 0,
            convergence_old_index: None,
            result: None,
            cancelled: false,
        })
    }

    pub fn adopt(&mut self, result: EditResult) -> Result<IncrementalDelta, IncrementalError> {
        if result.base_revision != self.revision {
            return Err(IncrementalError::Revision);
        }
        if result.before_hash32 != self.hash32() {
            return Err(IncrementalError::Hash);
        }
        let base_revision = self.revision;
        self.revision += 1;
        self.source = result.source;
        self.checkpoints = result.checkpoints;
        self.references = result.references;
        Ok(IncrementalDelta {
            base_revision,
            revision: self.revision,
            before_hash32: result.before_hash32,
            after_hash32: self.hash32(),
            restart_utf8: result.restart_utf8,
            convergence_utf8: result.convergence_utf8,
            reparsed_lines: result.reparsed_lines,
            reparsed_bytes: result.reparsed_bytes,
            replaced_checkpoints: result.replaced_checkpoints,
            inserted_checkpoints: result.inserted_checkpoints,
            reused_checkpoints: result.reused_checkpoints,
            references: result.reference_delta,
        })
    }

    pub fn assert_checkpoint_oracle(&self) {
        let oracle = CheckpointTree::from_source(&self.source);
        assert_eq!(
            self.checkpoints.signatures(),
            oracle.signatures(),
            "incremental checkpoint state diverged from clean scan"
        );
        assert_eq!(
            self.references.semantic_snapshot(),
            ReferenceIndex::from_tree(&oracle).semantic_snapshot(),
            "incremental reference index diverged from clean scan"
        );
    }
}

pub struct EditSession {
    base_revision: u64,
    before_hash32: u32,
    old_edit_end: usize,
    new_edit_end: usize,
    byte_delta: isize,
    old_checkpoints: CheckpointTree,
    references: ReferenceIndex,
    edited: SourceRope,
    restart_index: usize,
    restart_offset: usize,
    offset: usize,
    state: BlockState,
    inserted: Vec<Arc<LineRecord>>,
    reparsed_lines: usize,
    reparsed_bytes: usize,
    convergence_old_index: Option<usize>,
    result: Option<EditResult>,
    cancelled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvanceStatus {
    Pending,
    Converged,
    Cancelled,
}

impl EditSession {
    pub fn advance(&mut self, budget: WorkBudget, cancel: &CancelFlag) -> AdvanceStatus {
        if self.result.is_some() {
            return AdvanceStatus::Converged;
        }
        if self.cancelled || cancel.is_cancelled() {
            self.cancelled = true;
            return AdvanceStatus::Cancelled;
        }
        let mut lines = 0;
        let mut bytes = 0;
        while self.offset < self.edited.len()
            && (lines == 0 || (lines < budget.max_lines && bytes < budget.max_bytes))
        {
            if cancel.is_cancelled() {
                self.cancelled = true;
                return AdvanceStatus::Cancelled;
            }
            let (end, line) = self.edited.line_from(self.offset);
            let facts = scan_line_facts(&line, &self.state);
            self.state = advance_state(self.state.clone(), &line);
            let record = Arc::new(LineRecord {
                len: end - self.offset,
                hash: hash64(line.as_bytes()),
                state_after: self.state.clone(),
                order_key: 0,
                facts,
            });
            self.inserted.push(record.clone());
            let line_bytes = end - self.offset;
            self.offset = end;
            self.reparsed_lines += 1;
            self.reparsed_bytes += line_bytes;
            lines += 1;
            bytes += line_bytes;

            if self.offset >= self.new_edit_end {
                let mapped = self.offset.checked_add_signed(-self.byte_delta);
                if let Some(mapped) = mapped.filter(|mapped| *mapped >= self.old_edit_end) {
                    if let Some(boundary) = self.old_checkpoints.index_at_boundary(mapped) {
                        if boundary > 0
                            && same_record_semantics(
                                &self.old_checkpoints.record_at(boundary - 1),
                                &record,
                            )
                        {
                            self.inserted.pop();
                            self.convergence_old_index = Some(boundary - 1);
                            self.finish_result();
                            return AdvanceStatus::Converged;
                        }
                    }
                }
            }
        }

        if self.offset == self.edited.len() {
            self.finish_result();
            AdvanceStatus::Converged
        } else {
            AdvanceStatus::Pending
        }
    }

    pub fn into_result(self) -> Result<EditResult, IncrementalError> {
        if self.cancelled {
            return Err(IncrementalError::NotConverged);
        }
        self.result.ok_or(IncrementalError::NotConverged)
    }

    fn finish_result(&mut self) {
        if self.result.is_some() {
            return;
        }
        let old_end_index = self
            .convergence_old_index
            .unwrap_or_else(|| self.old_checkpoints.records());
        let lower_order_key = if self.restart_index == 0 {
            0
        } else {
            self.old_checkpoints
                .record_at(self.restart_index - 1)
                .order_key
        };
        let upper_order_key = if old_end_index == self.old_checkpoints.records() {
            u128::MAX
        } else {
            self.old_checkpoints.record_at(old_end_index).order_key
        };
        let spacing = (upper_order_key - lower_order_key) / (self.inserted.len() as u128 + 1);
        assert!(spacing > 0, "semantic order-key space exhausted");
        self.inserted = self
            .inserted
            .iter()
            .enumerate()
            .map(|(index, record)| {
                let mut record = (**record).clone();
                record.order_key = lower_order_key + spacing * (index as u128 + 1);
                Arc::new(record)
            })
            .collect();
        let removed = self
            .old_checkpoints
            .records_in_range(self.restart_index, old_end_index);
        let reference_delta = self.references.apply(&removed, &self.inserted);
        let inserted_tree = CheckpointTree::from_records(self.inserted.clone());
        let checkpoints =
            self.old_checkpoints
                .replace(self.restart_index, old_end_index, inserted_tree);
        assert_eq!(checkpoints.bytes(), self.edited.len());
        let convergence_utf8 = if let Some(old_index) = self.convergence_old_index {
            self.old_checkpoints
                .prefix_bytes(old_index + 1)
                .checked_add_signed(self.byte_delta)
                .unwrap()
        } else {
            self.edited.len()
        };
        let replaced = old_end_index - self.restart_index;
        let inserted = self.inserted.len();
        let reused = self.old_checkpoints.records() - replaced;
        self.result = Some(EditResult {
            base_revision: self.base_revision,
            before_hash32: self.before_hash32,
            source: self.edited.clone(),
            checkpoints,
            restart_utf8: self.restart_offset,
            convergence_utf8,
            reparsed_lines: self.reparsed_lines,
            reparsed_bytes: self.reparsed_bytes,
            replaced_checkpoints: replaced,
            inserted_checkpoints: inserted,
            reused_checkpoints: reused,
            references: self.references.clone(),
            reference_delta,
        });
    }
}

pub struct EditResult {
    base_revision: u64,
    before_hash32: u32,
    source: SourceRope,
    checkpoints: CheckpointTree,
    restart_utf8: usize,
    convergence_utf8: usize,
    reparsed_lines: usize,
    reparsed_bytes: usize,
    replaced_checkpoints: usize,
    inserted_checkpoints: usize,
    reused_checkpoints: usize,
    references: ReferenceIndex,
    reference_delta: ReferenceDelta,
}

#[derive(Clone, Debug)]
pub struct IncrementalDelta {
    pub base_revision: u64,
    pub revision: u64,
    pub before_hash32: u32,
    pub after_hash32: u32,
    pub restart_utf8: usize,
    pub convergence_utf8: usize,
    pub reparsed_lines: usize,
    pub reparsed_bytes: usize,
    pub replaced_checkpoints: usize,
    pub inserted_checkpoints: usize,
    pub reused_checkpoints: usize,
    pub references: ReferenceDelta,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReferenceDelta {
    pub presence_changed: Vec<String>,
    pub value_changed: Vec<String>,
    pub invalidated_lookup_records: usize,
}

fn same_record_semantics(left: &LineRecord, right: &LineRecord) -> bool {
    left.len == right.len
        && left.hash == right.hash
        && left.state_after == right.state_after
        && left.facts == right.facts
}

fn hash64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_tree_replaces_and_reuses_suffix() {
        let source = SourceRope::from_str("a\nb\nc\nd\n");
        let tree = CheckpointTree::from_source(&source);
        let old_last = tree.record_at(3);
        let inserted = CheckpointTree::from_source(&SourceRope::from_str("x\ny\n"));
        let replaced = tree.replace(1, 3, inserted);
        assert_eq!(replaced.bytes(), source.len());
        assert_eq!(replaced.records(), 4);
        assert!(Arc::ptr_eq(&replaced.record_at(3), &old_last));
    }
}
