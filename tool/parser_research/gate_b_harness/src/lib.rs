//! Donor-neutral executable contract for RFC 023 Gate B.
//!
//! A candidate implements [`GateBEngine`]. This crate pins the inline and
//! reference grammar profile, editor histories, segmented source-map model,
//! direct output/delta contract, fuel/cancellation rules, and resource gates.
//! It deliberately contains no candidate parser.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::ops::Range;
use std::path::Path;

use serde::Deserialize;

pub const COMMONMARK_VERSION: &str = "0.31.2";
pub const GFM_VERSION: &str = "0.29";
pub const POLL_FUEL_BYTES: usize = 4 * 1024;
pub const POLL_FUEL_TRANSITIONS: usize = 4 * 1024;

pub const COMMONMARK_GATE_B_SECTIONS: &[&str] = &[
    "Backslash escapes",
    "Entity and numeric character references",
    "Code spans",
    "Emphasis and strong emphasis",
    "Links",
    "Images",
    "Autolinks",
    "Raw HTML",
    "Hard line breaks",
    "Soft line breaks",
    "Textual content",
    "Link reference definitions",
    "Inlines",
    "Precedence",
];

pub const GFM_GATE_B_SECTIONS: &[&str] = &[
    "Autolinks (extension)",
    "Strikethrough (extension)",
    "Disallowed Raw HTML (extension)",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxProfile {
    CommonMark0312,
    FlarkGfm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FixtureAuthority {
    CommonMark0312,
    Gfm029,
}

impl FixtureAuthority {
    pub const fn profile(self) -> SyntaxProfile {
        match self {
            Self::CommonMark0312 => SyntaxProfile::CommonMark0312,
            Self::Gfm029 => SyntaxProfile::FlarkGfm,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SpecFixture {
    pub markdown: String,
    pub html: String,
    pub example: usize,
    pub section: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateFixture {
    pub authority: FixtureAuthority,
    pub fixture: SpecFixture,
}

pub fn load_gate_b_fixtures(repo_root: &Path) -> Result<Vec<GateFixture>, String> {
    let commonmark =
        load_fixtures(&repo_root.join("test/fixtures/commonmark/upstream/common_mark_tests.json"))?;
    let gfm = load_fixtures(&repo_root.join("test/fixtures/commonmark/upstream/gfm_tests.json"))?;
    let mut selected = commonmark
        .into_iter()
        .filter(|fixture| COMMONMARK_GATE_B_SECTIONS.contains(&fixture.section.as_str()))
        .map(|fixture| GateFixture {
            authority: FixtureAuthority::CommonMark0312,
            fixture,
        })
        .collect::<Vec<_>>();
    selected.extend(
        gfm.into_iter()
            .filter(|fixture| GFM_GATE_B_SECTIONS.contains(&fixture.section.as_str()))
            .map(|fixture| GateFixture {
                authority: FixtureAuthority::Gfm029,
                fixture,
            }),
    );
    Ok(selected)
}

fn load_fixtures(path: &Path) -> Result<Vec<SpecFixture>, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FixtureReport {
    pub passed: usize,
    pub by_section: BTreeMap<String, usize>,
}

pub fn run_fixture_corpus<E: GateBEngine>(repo_root: &Path) -> Result<FixtureReport, String> {
    let fixtures = load_gate_b_fixtures(repo_root)?;
    let mut report = FixtureReport::default();
    for gate in fixtures {
        let profile = gate.authority.profile();
        let engine = E::open(profile, &gate.fixture.markdown).map_err(engine_error)?;
        let snapshot = engine.snapshot().map_err(engine_error)?;
        validate_snapshot(&gate.fixture.markdown, &snapshot)?;
        if snapshot.test_html != gate.fixture.html {
            return Err(format!(
                "{:?} example {} ({}) differs from normative HTML",
                gate.authority, gate.fixture.example, gate.fixture.section
            ));
        }
        let clean = E::clean_snapshot(profile, &gate.fixture.markdown).map_err(engine_error)?;
        validate_clean_equivalence(&snapshot, &clean)?;
        report.passed += 1;
        *report.by_section.entry(gate.fixture.section).or_default() += 1;
    }
    Ok(report)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub base_revision: u64,
    pub start_utf8: usize,
    pub end_utf8: usize,
    pub replacement: String,
}

impl Edit {
    pub fn apply(&self, source: &str) -> Result<String, String> {
        if self.start_utf8 > self.end_utf8 || self.end_utf8 > source.len() {
            return Err(format!(
                "invalid edit range {}..{} for {} bytes",
                self.start_utf8,
                self.end_utf8,
                source.len()
            ));
        }
        if !source.is_char_boundary(self.start_utf8) || !source.is_char_boundary(self.end_utf8) {
            return Err("edit range splits a UTF-8 scalar".to_owned());
        }
        if self.replacement.contains('\r') {
            return Err("Gate B sources are LF-normalized".to_owned());
        }
        let mut edited = source.to_owned();
        edited.replace_range(self.start_utf8..self.end_utf8, &self.replacement);
        Ok(edited)
    }
}

pub fn minimal_edit(base_revision: u64, before: &str, after: &str) -> Edit {
    let mut prefix = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while prefix > 0 && (!before.is_char_boundary(prefix) || !after.is_char_boundary(prefix)) {
        prefix -= 1;
    }

    let before_tail = &before[prefix..];
    let after_tail = &after[prefix..];
    let suffix = before_tail
        .bytes()
        .rev()
        .zip(after_tail.bytes().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let mut before_end = before.len() - suffix;
    let mut after_end = after.len() - suffix;
    while before_end < before.len()
        && after_end < after.len()
        && (!before.is_char_boundary(before_end) || !after.is_char_boundary(after_end))
    {
        before_end += 1;
        after_end += 1;
    }
    Edit {
        base_revision,
        start_utf8: prefix,
        end_utf8: before_end,
        replacement: after[prefix..after_end].to_owned(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StableId(pub u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualReason {
    ContainerLineJoin,
    CodeSpanLineNormalization,
    TabExpansion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapPiece {
    Source(Range<usize>),
    Virtual {
        anchor_utf8: usize,
        text: String,
        reason: VirtualReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentedText {
    /// Test-only materialization used for equality and mapping validation.
    /// Production leaves/runs must retain source pieces plus compact virtual
    /// descriptors, not duplicate owned strings for every record.
    pub text: String,
    pub pieces: Vec<MapPiece>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineLeaf {
    pub id: StableId,
    /// Physical envelope. Prefix bytes may occur between mapped pieces.
    pub source: Range<usize>,
    /// The exact logical input presented to the inline machine.
    pub input: SegmentedText,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutolinkKind {
    Uri,
    Email,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutolinkForm {
    Angle,
    Bare,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkForm {
    Inline,
    FullReference,
    CollapsedReference,
    ShortcutReference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayPolicy {
    Rendered,
    SourceVisible,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InlineKind {
    Text,
    Escape,
    Entity,
    CodeSpan,
    Emphasis,
    Strong,
    Strikethrough,
    Link {
        form: LinkForm,
    },
    Image {
        form: LinkForm,
    },
    Autolink {
        kind: AutolinkKind,
        form: AutolinkForm,
    },
    RawHtml {
        display: DisplayPolicy,
    },
    SoftBreak,
    HardBreak,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactTag {
    Text,
    CodeSpan,
    Emphasis,
    Strong,
    Strikethrough,
    Link,
    Image,
    Autolink,
    RawHtml,
    SoftBreak,
    HardBreak,
}

impl InlineKind {
    pub const fn tag(&self) -> FactTag {
        match self {
            Self::Text | Self::Escape | Self::Entity => FactTag::Text,
            Self::CodeSpan => FactTag::CodeSpan,
            Self::Emphasis => FactTag::Emphasis,
            Self::Strong => FactTag::Strong,
            Self::Strikethrough => FactTag::Strikethrough,
            Self::Link { .. } => FactTag::Link,
            Self::Image { .. } => FactTag::Image,
            Self::Autolink { .. } => FactTag::Autolink,
            Self::RawHtml { .. } => FactTag::RawHtml,
            Self::SoftBreak => FactTag::SoftBreak,
            Self::HardBreak => FactTag::HardBreak,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineFact {
    pub id: StableId,
    pub leaf: StableId,
    pub parent: Option<StableId>,
    pub kind: InlineKind,
    pub source: Range<usize>,
    pub markers: Vec<Range<usize>>,
    /// User-visible semantic content. Delimiters are excluded.
    pub content: SegmentedText,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceDefinition {
    pub symbol: StableId,
    pub normalized_label: String,
    pub destination: String,
    pub title: Option<String>,
    pub source: Range<usize>,
    pub markers: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceUse {
    pub fact: StableId,
    pub symbol: StableId,
    pub label_source: Range<usize>,
}

/// Immutable semantic output root for one inline leaf. The root addresses
/// candidate-owned chunked output; a delta can adopt a new root without
/// serializing every fact or reference use across the worker boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafOutput {
    pub leaf: StableId,
    pub root: StableId,
    pub fact_count: usize,
    pub reference_use_count: usize,
}

/// A dependency exists even while its label is undefined. Retaining misses is
/// what lets adding a definition re-resolve affected leaves without a global
/// grammar scan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelDependency {
    pub leaf: StableId,
    pub normalized_label: String,
    pub occurrences: usize,
    pub resolved_symbol: Option<StableId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyIndexState {
    pub normalized_label: String,
    pub generation: u64,
    pub dependent_leaf_count: usize,
    pub occurrences: usize,
    pub resolved_symbol: Option<StableId>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LabelDependencyKey {
    pub leaf: StableId,
    pub normalized_label: String,
}

impl LabelDependency {
    pub fn key(&self) -> LabelDependencyKey {
        LabelDependencyKey {
            leaf: self.leaf,
            normalized_label: self.normalized_label.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResourceMetrics {
    /// All source storage concurrently retained by the committed revision and
    /// pending candidate, with shared rope pages counted once.
    pub source_backing_bytes: usize,
    /// Diagnostic split of the four output/index categories below. Their sum
    /// must equal leaf + inline + map + reference bytes; they are not added to
    /// the total a second time.
    pub committed_output_bytes: usize,
    pub pending_output_bytes: usize,
    pub checkpoint_bytes: usize,
    pub block_context_bytes: usize,
    pub lexical_tape_bytes: usize,
    pub resolution_stack_bytes: usize,
    pub leaf_index_bytes: usize,
    pub inline_output_bytes: usize,
    pub source_map_bytes: usize,
    pub reference_index_bytes: usize,
    pub retained_history_bytes: usize,
    pub retained_source_prefix_bytes: usize,
    pub peak_scratch_bytes: usize,
    /// Exact sum of every non-overlapping live category plus peak scratch.
    /// This is the cap used by the gate; category fields make it auditable.
    pub total_peak_live_bytes: usize,
}

impl ResourceMetrics {
    pub fn persistent_auxiliary_bytes(&self) -> usize {
        self.checkpoint_bytes
            .saturating_add(self.block_context_bytes)
            .saturating_add(self.lexical_tape_bytes)
            .saturating_add(self.resolution_stack_bytes)
            .saturating_add(self.leaf_index_bytes)
            .saturating_add(self.inline_output_bytes)
            .saturating_add(self.source_map_bytes)
            .saturating_add(self.reference_index_bytes)
            .saturating_add(self.retained_history_bytes)
    }

    pub fn accounted_peak_live_bytes(&self) -> usize {
        self.source_backing_bytes
            .saturating_add(self.persistent_auxiliary_bytes())
            .saturating_add(self.peak_scratch_bytes)
    }

    pub fn categorized_output_bytes(&self) -> usize {
        self.leaf_index_bytes
            .saturating_add(self.inline_output_bytes)
            .saturating_add(self.source_map_bytes)
            .saturating_add(self.reference_index_bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub profile: SyntaxProfile,
    pub revision: u64,
    pub source_len: usize,
    pub source_hash64: u64,
    /// Test-only fixture serialization; never a production delta path.
    pub test_html: String,
    pub leaves: Vec<InlineLeaf>,
    pub facts: Vec<InlineFact>,
    pub definitions: Vec<ReferenceDefinition>,
    pub reference_uses: Vec<ReferenceUse>,
    pub leaf_outputs: Vec<LeafOutput>,
    /// Root of the persistent leaf-output sequence. Deltas adopt a new root;
    /// Dart requests only visible leaf outputs from that root.
    pub output_sequence_root: StableId,
    pub label_dependencies: Vec<LabelDependency>,
    pub dependency_indexes: Vec<DependencyIndexState>,
    pub resources: ResourceMetrics,
}

pub fn hash64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub fn validate_snapshot(source: &str, snapshot: &Snapshot) -> Result<(), String> {
    if snapshot.source_len != source.len() {
        return Err(format!(
            "snapshot source length {} != {}",
            snapshot.source_len,
            source.len()
        ));
    }
    if snapshot.source_hash64 != hash64(source.as_bytes()) {
        return Err("snapshot source hash mismatch".to_owned());
    }

    let mut leaf_ids = BTreeSet::new();
    for leaf in &snapshot.leaves {
        if !leaf_ids.insert(leaf.id) {
            return Err(format!("duplicate leaf ID {:?}", leaf.id));
        }
        validate_range(source, &leaf.source, "leaf")?;
        validate_segmented_text(source, &leaf.source, &leaf.input, "leaf input")?;
    }

    let mut fact_ids = BTreeSet::new();
    let facts_by_id = snapshot
        .facts
        .iter()
        .map(|fact| (fact.id, fact))
        .collect::<BTreeMap<_, _>>();
    for fact in &snapshot.facts {
        if !fact_ids.insert(fact.id) {
            return Err(format!("duplicate fact ID {:?}", fact.id));
        }
        let leaf = snapshot
            .leaves
            .iter()
            .find(|leaf| leaf.id == fact.leaf)
            .ok_or_else(|| format!("fact {:?} has missing leaf {:?}", fact.id, fact.leaf))?;
        validate_range(source, &fact.source, "fact")?;
        if fact.source.start < leaf.source.start || fact.source.end > leaf.source.end {
            return Err(format!(
                "fact {:?} range {:?} escapes leaf {:?}",
                fact.id, fact.source, leaf.source
            ));
        }
        for marker in &fact.markers {
            validate_range(source, marker, "marker")?;
            if marker.is_empty() {
                return Err(format!(
                    "marker on fact {:?} has no physical source bytes",
                    fact.id
                ));
            }
            if marker.start < fact.source.start || marker.end > fact.source.end {
                return Err(format!("marker {marker:?} escapes fact {:?}", fact.id));
            }
        }
        validate_segmented_text(source, &fact.source, &fact.content, "fact content")?;
        match &fact.kind {
            InlineKind::Autolink {
                form: AutolinkForm::Bare,
                ..
            } if !fact.markers.is_empty() => {
                return Err(format!(
                    "bare autolink {:?} must not invent hidden markers",
                    fact.id
                ));
            }
            InlineKind::Autolink {
                form: AutolinkForm::Angle,
                ..
            } if fact.markers.len() != 2 => {
                return Err(format!(
                    "angle autolink {:?} must expose both angle markers",
                    fact.id
                ));
            }
            InlineKind::RawHtml { display } if *display != DisplayPolicy::SourceVisible => {
                return Err(format!("raw HTML fact {:?} is not source-visible", fact.id));
            }
            _ => {}
        }
    }

    let leaf_order = snapshot
        .leaves
        .iter()
        .enumerate()
        .map(|(index, leaf)| (leaf.id, index))
        .collect::<BTreeMap<_, _>>();
    let mut last_leaf_index = 0usize;
    for (fact_index, fact) in snapshot.facts.iter().enumerate() {
        let index = leaf_order[&fact.leaf];
        if fact_index > 0 && index < last_leaf_index {
            return Err("inline facts are not grouped in leaf order".to_owned());
        }
        last_leaf_index = index;
    }

    for fact in &snapshot.facts {
        let mut ancestors = BTreeSet::new();
        let mut parent = fact.parent;
        while let Some(parent_id) = parent {
            if parent_id == fact.id || !ancestors.insert(parent_id) {
                return Err(format!("fact {:?} has cyclic parentage", fact.id));
            }
            let owner = facts_by_id
                .get(&parent_id)
                .ok_or_else(|| format!("fact {:?} has missing parent {parent_id:?}", fact.id))?;
            if owner.leaf != fact.leaf
                || owner.source.start > fact.source.start
                || fact.source.end > owner.source.end
            {
                return Err(format!(
                    "fact {:?} escapes semantic parent {parent_id:?}",
                    fact.id
                ));
            }
            parent = owner.parent;
        }
    }

    let mut definitions = BTreeMap::new();
    for definition in &snapshot.definitions {
        validate_range(source, &definition.source, "reference definition")?;
        if definition.normalized_label.starts_with('^') {
            return Err("footnote-shaped definition entered the reference table".to_owned());
        }
        for marker in &definition.markers {
            validate_range(source, marker, "reference-definition marker")?;
            if marker.is_empty() {
                return Err(format!(
                    "marker on definition {:?} has no physical source bytes",
                    definition.symbol
                ));
            }
            if marker.start < definition.source.start || marker.end > definition.source.end {
                return Err(format!(
                    "reference marker {marker:?} escapes {:?}",
                    definition.source
                ));
            }
        }
        if definitions.insert(definition.symbol, definition).is_some() {
            return Err(format!(
                "duplicate reference symbol {:?}",
                definition.symbol
            ));
        }
    }

    let mut use_facts = BTreeSet::new();
    for reference_use in &snapshot.reference_uses {
        let fact = facts_by_id.get(&reference_use.fact).ok_or_else(|| {
            format!(
                "reference use points to missing fact {:?}",
                reference_use.fact
            )
        })?;
        if !matches!(
            fact.kind,
            InlineKind::Link { .. } | InlineKind::Image { .. }
        ) {
            return Err(format!(
                "reference use fact {:?} is not a link/image",
                fact.id
            ));
        }
        if !definitions.contains_key(&reference_use.symbol) {
            return Err(format!(
                "reference use points to missing symbol {:?}",
                reference_use.symbol
            ));
        }
        validate_range(source, &reference_use.label_source, "reference label")?;
        if reference_use.label_source.start < fact.source.start
            || reference_use.label_source.end > fact.source.end
        {
            return Err(format!(
                "reference label {:?} escapes fact {:?}",
                reference_use.label_source, fact.id
            ));
        }
        if !use_facts.insert(fact.id) {
            return Err(format!("fact {:?} has duplicate reference uses", fact.id));
        }
    }

    let mut output_leaves = BTreeSet::new();
    let mut output_roots = BTreeSet::new();
    for output in &snapshot.leaf_outputs {
        if !leaf_ids.contains(&output.leaf) || !output_leaves.insert(output.leaf) {
            return Err(format!(
                "leaf output has missing/duplicate leaf {:?}",
                output.leaf
            ));
        }
        if !output_roots.insert(output.root) {
            return Err(format!("duplicate leaf output root {:?}", output.root));
        }
        let fact_count = snapshot
            .facts
            .iter()
            .filter(|fact| fact.leaf == output.leaf)
            .count();
        let use_count = snapshot
            .reference_uses
            .iter()
            .filter(|reference_use| {
                facts_by_id
                    .get(&reference_use.fact)
                    .is_some_and(|fact| fact.leaf == output.leaf)
            })
            .count();
        if output.fact_count != fact_count || output.reference_use_count != use_count {
            return Err(format!(
                "leaf output {:?} count metadata is stale",
                output.root
            ));
        }
    }
    if output_leaves != leaf_ids {
        return Err("every inline leaf must expose one immutable output root".to_owned());
    }

    let mut dependency_keys = BTreeSet::new();
    for dependency in &snapshot.label_dependencies {
        if !leaf_ids.contains(&dependency.leaf) {
            return Err(format!(
                "label dependency has missing leaf {:?}",
                dependency.leaf
            ));
        }
        if dependency.occurrences == 0 || dependency.normalized_label.starts_with('^') {
            return Err(format!("invalid label dependency {dependency:?}"));
        }
        if !dependency_keys.insert(dependency.key()) {
            return Err(format!("duplicate label dependency {:?}", dependency.key()));
        }
        if let Some(symbol) = dependency.resolved_symbol {
            let definition = definitions
                .get(&symbol)
                .ok_or_else(|| format!("dependency points to stale symbol {symbol:?}"))?;
            if definition.normalized_label != dependency.normalized_label {
                return Err(
                    "dependency symbol label does not match its normalized label".to_owned(),
                );
            }
        }
    }
    let mut index_labels = BTreeSet::new();
    for index in &snapshot.dependency_indexes {
        if index.normalized_label.starts_with('^')
            || !index_labels.insert(index.normalized_label.clone())
        {
            return Err(format!("invalid/duplicate dependency index {index:?}"));
        }
        let dependencies = snapshot
            .label_dependencies
            .iter()
            .filter(|dependency| dependency.normalized_label == index.normalized_label)
            .collect::<Vec<_>>();
        let leaves = dependencies
            .iter()
            .map(|dependency| dependency.leaf)
            .collect::<BTreeSet<_>>();
        let occurrences = dependencies
            .iter()
            .map(|dependency| dependency.occurrences)
            .sum::<usize>();
        if dependencies.is_empty()
            || leaves.len() != index.dependent_leaf_count
            || occurrences != index.occurrences
            || dependencies
                .iter()
                .any(|dependency| dependency.resolved_symbol != index.resolved_symbol)
        {
            return Err(format!("dependency index metadata is stale: {index:?}"));
        }
        if let Some(symbol) = index.resolved_symbol {
            let definition = definitions
                .get(&symbol)
                .ok_or_else(|| format!("dependency index has stale symbol {symbol:?}"))?;
            if definition.normalized_label != index.normalized_label {
                return Err("dependency index symbol label mismatch".to_owned());
            }
        }
    }
    if snapshot
        .label_dependencies
        .iter()
        .any(|dependency| !index_labels.contains(&dependency.normalized_label))
    {
        return Err("label dependency is missing its global index state".to_owned());
    }
    for reference_use in &snapshot.reference_uses {
        let fact = facts_by_id[&reference_use.fact];
        let definition = definitions[&reference_use.symbol];
        if !snapshot.label_dependencies.iter().any(|dependency| {
            dependency.leaf == fact.leaf
                && dependency.normalized_label == definition.normalized_label
                && dependency.resolved_symbol == Some(reference_use.symbol)
        }) {
            return Err(format!(
                "reference use {:?} has no resolved leaf dependency",
                reference_use.fact
            ));
        }
    }
    Ok(())
}

fn validate_range(source: &str, range: &Range<usize>, label: &str) -> Result<(), String> {
    if range.start > range.end || range.end > source.len() {
        return Err(format!("invalid {label} range {range:?}"));
    }
    if !source.is_char_boundary(range.start) || !source.is_char_boundary(range.end) {
        return Err(format!("{label} range {range:?} splits UTF-8"));
    }
    Ok(())
}

fn validate_segmented_text(
    source: &str,
    envelope: &Range<usize>,
    mapped: &SegmentedText,
    label: &str,
) -> Result<(), String> {
    let mut reconstructed = String::new();
    let mut last_source_end = envelope.start;
    let mut last_physical_position = envelope.start;
    for piece in &mapped.pieces {
        match piece {
            MapPiece::Source(range) => {
                validate_range(source, range, label)?;
                if range.start < envelope.start || range.end > envelope.end {
                    return Err(format!("{label} piece {range:?} escapes {envelope:?}"));
                }
                if range.start < last_source_end || range.start < last_physical_position {
                    return Err(format!("{label} source pieces overlap or go backwards"));
                }
                reconstructed.push_str(&source[range.clone()]);
                last_source_end = range.end;
                last_physical_position = range.end;
            }
            MapPiece::Virtual {
                anchor_utf8,
                text,
                reason,
            } => {
                if *anchor_utf8 < envelope.start || *anchor_utf8 > envelope.end {
                    return Err(format!("{label} virtual anchor escapes {envelope:?}"));
                }
                if !source.is_char_boundary(*anchor_utf8) {
                    return Err(format!("{label} virtual anchor splits UTF-8"));
                }
                if *anchor_utf8 < last_physical_position {
                    return Err(format!("{label} virtual anchors go backwards"));
                }
                last_physical_position = *anchor_utf8;
                let valid = match reason {
                    VirtualReason::ContainerLineJoin => text == "\n",
                    VirtualReason::CodeSpanLineNormalization => text == " ",
                    VirtualReason::TabExpansion => {
                        !text.is_empty() && text.len() <= 4 && text.bytes().all(|byte| byte == b' ')
                    }
                };
                if !valid {
                    return Err(format!("{label} has invalid {reason:?} text {text:?}"));
                }
                reconstructed.push_str(text);
            }
        }
    }
    if reconstructed != mapped.text {
        return Err(format!(
            "{label} mapping reconstructs {reconstructed:?}, expected {:?}",
            mapped.text
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CanonicalFact {
    leaf: usize,
    parent: Option<usize>,
    kind: InlineKind,
    source: Range<usize>,
    markers: Vec<Range<usize>>,
    content: SegmentedText,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CanonicalUse {
    fact: usize,
    normalized_label: String,
    label_source: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CanonicalDefinition {
    normalized_label: String,
    destination: String,
    title: Option<String>,
    source: Range<usize>,
    markers: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CanonicalDependency {
    leaf: usize,
    normalized_label: String,
    occurrences: usize,
    resolved_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CanonicalDependencyIndex {
    normalized_label: String,
    dependent_leaf_count: usize,
    occurrences: usize,
    resolved_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CanonicalSnapshot {
    profile: SyntaxProfile,
    source_len: usize,
    source_hash64: u64,
    test_html: String,
    leaves: Vec<(Range<usize>, SegmentedText)>,
    facts: Vec<CanonicalFact>,
    definitions: Vec<CanonicalDefinition>,
    uses: Vec<CanonicalUse>,
    dependencies: Vec<CanonicalDependency>,
    dependency_indexes: Vec<CanonicalDependencyIndex>,
}

fn canonicalize(snapshot: &Snapshot) -> Result<CanonicalSnapshot, String> {
    let leaf_indexes = snapshot
        .leaves
        .iter()
        .enumerate()
        .map(|(index, leaf)| (leaf.id, index))
        .collect::<BTreeMap<_, _>>();
    let fact_indexes = snapshot
        .facts
        .iter()
        .enumerate()
        .map(|(index, fact)| (fact.id, index))
        .collect::<BTreeMap<_, _>>();
    let definitions = snapshot
        .definitions
        .iter()
        .map(|definition| (definition.symbol, definition.normalized_label.clone()))
        .collect::<BTreeMap<_, _>>();
    Ok(CanonicalSnapshot {
        profile: snapshot.profile,
        source_len: snapshot.source_len,
        source_hash64: snapshot.source_hash64,
        test_html: snapshot.test_html.clone(),
        leaves: snapshot
            .leaves
            .iter()
            .map(|leaf| (leaf.source.clone(), leaf.input.clone()))
            .collect(),
        facts: snapshot
            .facts
            .iter()
            .map(|fact| {
                Ok(CanonicalFact {
                    leaf: *leaf_indexes
                        .get(&fact.leaf)
                        .ok_or_else(|| format!("missing leaf {:?}", fact.leaf))?,
                    parent: fact
                        .parent
                        .map(|parent| {
                            fact_indexes
                                .get(&parent)
                                .copied()
                                .ok_or_else(|| format!("missing parent {parent:?}"))
                        })
                        .transpose()?,
                    kind: fact.kind.clone(),
                    source: fact.source.clone(),
                    markers: fact.markers.clone(),
                    content: fact.content.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        definitions: snapshot
            .definitions
            .iter()
            .map(|definition| CanonicalDefinition {
                normalized_label: definition.normalized_label.clone(),
                destination: definition.destination.clone(),
                title: definition.title.clone(),
                source: definition.source.clone(),
                markers: definition.markers.clone(),
            })
            .collect(),
        uses: snapshot
            .reference_uses
            .iter()
            .map(|reference_use| {
                Ok(CanonicalUse {
                    fact: *fact_indexes
                        .get(&reference_use.fact)
                        .ok_or_else(|| "reference use has missing fact".to_owned())?,
                    normalized_label: definitions
                        .get(&reference_use.symbol)
                        .ok_or_else(|| "reference use has missing symbol".to_owned())?
                        .clone(),
                    label_source: reference_use.label_source.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        dependencies: snapshot
            .label_dependencies
            .iter()
            .map(|dependency| {
                Ok(CanonicalDependency {
                    leaf: *leaf_indexes
                        .get(&dependency.leaf)
                        .ok_or_else(|| "dependency has missing leaf".to_owned())?,
                    normalized_label: dependency.normalized_label.clone(),
                    occurrences: dependency.occurrences,
                    resolved_label: dependency
                        .resolved_symbol
                        .map(|symbol| {
                            definitions
                                .get(&symbol)
                                .cloned()
                                .ok_or_else(|| "dependency has stale symbol".to_owned())
                        })
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        dependency_indexes: snapshot
            .dependency_indexes
            .iter()
            .map(|index| {
                Ok(CanonicalDependencyIndex {
                    normalized_label: index.normalized_label.clone(),
                    dependent_leaf_count: index.dependent_leaf_count,
                    occurrences: index.occurrences,
                    resolved_label: index
                        .resolved_symbol
                        .map(|symbol| {
                            definitions
                                .get(&symbol)
                                .cloned()
                                .ok_or_else(|| "dependency index has stale symbol".to_owned())
                        })
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    })
}

pub fn validate_clean_equivalence(incremental: &Snapshot, clean: &Snapshot) -> Result<(), String> {
    if canonicalize(incremental)? != canonicalize(clean)? {
        return Err("incremental snapshot differs from candidate-clean parse".to_owned());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkFuel {
    pub max_source_bytes: usize,
    pub max_transitions: usize,
}

impl WorkFuel {
    pub const GATE_B: Self = Self {
        max_source_bytes: POLL_FUEL_BYTES,
        max_transitions: POLL_FUEL_TRANSITIONS,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollStatus {
    Pending,
    Ready,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditEvent {
    SourceSpanExamined(Range<usize>),
    /// Every grammar transition is fuelled, including delimiter/bracket
    /// resolution and final output sealing. A candidate cannot stop charging
    /// fuel when byte scanning ends.
    GrammarTransitions {
        kind: TransitionKind,
        count: usize,
    },
    CancellationCheck,
    Allocation(usize),
    GeneralBatchTreeMaterialization,
    GrammarSideScan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionKind {
    Scan,
    DelimiterResolution,
    BracketResolution,
    ReferenceResolution,
    Emit,
    Finalize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhaseReceipt {
    pub source_bytes_examined: usize,
    pub grammar_transitions: usize,
    pub structural_steps: usize,
    pub newly_allocated_bytes: usize,
    pub general_batch_trees: usize,
    pub grammar_side_scans: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhaseEvidence {
    pub receipt: PhaseReceipt,
    pub events: Vec<AuditEvent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollReceipt {
    pub token: PendingToken,
    pub status: PollStatus,
    pub source_bytes_examined: usize,
    pub transitions: usize,
    pub cancellation_checks: usize,
    pub max_uninterrupted_transitions: usize,
    pub newly_allocated_bytes: usize,
    pub peak_scratch_bytes: usize,
    pub general_batch_trees: usize,
    pub grammar_side_scans: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PollEvidence {
    pub receipt: PollReceipt,
    pub events: Vec<AuditEvent>,
}

pub fn validate_phase_evidence(evidence: &PhaseEvidence, phase: &str) -> Result<(), String> {
    let counts = count_events(&evidence.events)?;
    let receipt = evidence.receipt;
    if counts.source_bytes != receipt.source_bytes_examined
        || counts.transitions != receipt.grammar_transitions
        || counts.allocations != receipt.newly_allocated_bytes
        || counts.batch_trees != receipt.general_batch_trees
        || counts.side_scans != receipt.grammar_side_scans
    {
        return Err(format!("{phase} receipt does not match its audit trace"));
    }
    if receipt.source_bytes_examined > 64 || receipt.grammar_transitions != 0 {
        return Err(format!("{phase} hid grammar work outside poll"));
    }
    if receipt.structural_steps > 256 || receipt.newly_allocated_bytes > 64 * 1024 {
        return Err(format!("{phase} did unbounded structural/allocation work"));
    }
    if receipt.general_batch_trees != 0 || receipt.grammar_side_scans != 0 {
        return Err(format!("{phase} used a batch tree or grammar side scan"));
    }
    Ok(())
}

pub fn validate_poll_evidence(
    evidence: &PollEvidence,
    fuel: WorkFuel,
    expected_token: PendingToken,
) -> Result<(), String> {
    if fuel.max_source_bytes == 0 || fuel.max_transitions == 0 {
        return Err("poll fuel must be non-zero".to_owned());
    }
    if fuel.max_source_bytes > POLL_FUEL_BYTES || fuel.max_transitions > POLL_FUEL_TRANSITIONS {
        return Err("Gate B polls may not exceed the 4 KiB product budget".to_owned());
    }
    let receipt = evidence.receipt;
    if receipt.token != expected_token {
        return Err(format!(
            "poll reported token {:?}, expected {expected_token:?}",
            receipt.token
        ));
    }
    let counts = count_events(&evidence.events)?;
    if counts.source_bytes != receipt.source_bytes_examined
        || counts.transitions != receipt.transitions
        || counts.cancellation_checks != receipt.cancellation_checks
        || counts.max_uninterrupted != receipt.max_uninterrupted_transitions
        || counts.allocations != receipt.newly_allocated_bytes
        || counts.batch_trees != receipt.general_batch_trees
        || counts.side_scans != receipt.grammar_side_scans
    {
        return Err("poll receipt does not match its ordered audit trace".to_owned());
    }
    if receipt.source_bytes_examined > fuel.max_source_bytes
        || receipt.transitions > fuel.max_transitions
        || receipt.max_uninterrupted_transitions > fuel.max_transitions
    {
        return Err("poll exceeded byte or transition fuel".to_owned());
    }
    if receipt.transitions > 0 && receipt.cancellation_checks == 0 {
        return Err("poll did work without a cancellation check".to_owned());
    }
    if receipt.status == PollStatus::Pending
        && receipt.source_bytes_examined == 0
        && receipt.transitions == 0
    {
        return Err("pending poll made no progress".to_owned());
    }
    if receipt.general_batch_trees != 0 || receipt.grammar_side_scans != 0 {
        return Err("poll used a general batch tree or grammar side scan".to_owned());
    }
    if receipt.status == PollStatus::Ready
        && !evidence.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::GrammarTransitions {
                    kind: TransitionKind::Finalize,
                    count
                } if *count > 0
            )
        })
    {
        return Err("ready poll did not fuel-charge finalization/fact sealing".to_owned());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default)]
struct EventCounts {
    source_bytes: usize,
    transitions: usize,
    cancellation_checks: usize,
    max_uninterrupted: usize,
    allocations: usize,
    batch_trees: usize,
    side_scans: usize,
}

fn count_events(events: &[AuditEvent]) -> Result<EventCounts, String> {
    let mut counts = EventCounts::default();
    let mut uninterrupted = 0usize;
    for event in events {
        match event {
            AuditEvent::SourceSpanExamined(range) => {
                if range.start > range.end {
                    return Err(format!("invalid audited source span {range:?}"));
                }
                counts.source_bytes = counts.source_bytes.saturating_add(range.len());
            }
            AuditEvent::GrammarTransitions { count, .. } => {
                counts.transitions = counts.transitions.saturating_add(*count);
                uninterrupted = uninterrupted.saturating_add(*count);
                counts.max_uninterrupted = counts.max_uninterrupted.max(uninterrupted);
            }
            AuditEvent::CancellationCheck => {
                counts.cancellation_checks += 1;
                uninterrupted = 0;
            }
            AuditEvent::Allocation(bytes) => {
                counts.allocations = counts.allocations.saturating_add(*bytes);
            }
            AuditEvent::GeneralBatchTreeMaterialization => counts.batch_trees += 1,
            AuditEvent::GrammarSideScan => counts.side_scans += 1,
        }
    }
    Ok(counts)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PendingToken {
    pub revision: u64,
    /// Monotonic even when several superseded attempts target one revision.
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeginEvidence {
    pub token: PendingToken,
    pub phase: PhaseEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupersedeEvidence {
    pub cancelled: PendingToken,
    pub started: BeginEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderSplice {
    /// Index in the sequence after all preceding splices in this delta.
    pub start: usize,
    pub removed: Vec<StableId>,
    pub inserted: Vec<StableId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputSequenceRefresh {
    pub removed_root: StableId,
    pub inserted_root: StableId,
    pub affected_leaf_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyIndexRefresh {
    pub normalized_label: String,
    pub removed_generation: Option<u64>,
    pub inserted_generation: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionDelta {
    pub base_revision: u64,
    pub revision: u64,
    pub removed_leaf_ids: Vec<StableId>,
    pub upserted_leaves: Vec<InlineLeaf>,
    pub leaf_order_splices: Vec<OrderSplice>,
    pub removed_fact_ids: Vec<StableId>,
    pub upserted_facts: Vec<InlineFact>,
    pub fact_order_splices: Vec<OrderSplice>,
    /// Compact adoption of a candidate-owned persistent output sequence. Facts
    /// and uses in changed leaves must not also be enumerated.
    pub output_sequence_refresh: Option<OutputSequenceRefresh>,
    pub removed_definition_symbols: Vec<StableId>,
    pub upserted_definitions: Vec<ReferenceDefinition>,
    pub removed_reference_use_facts: Vec<StableId>,
    pub upserted_reference_uses: Vec<ReferenceUse>,
    pub removed_label_dependencies: Vec<LabelDependencyKey>,
    pub upserted_label_dependencies: Vec<LabelDependency>,
    pub dependency_index_refreshes: Vec<DependencyIndexRefresh>,
    pub encoded_bytes: usize,
    pub general_batch_trees: usize,
    pub grammar_side_scans: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitEvidence {
    pub token: PendingToken,
    pub delta: RevisionDelta,
    pub phase: PhaseEvidence,
}

/// Production adapter contract. `open`, `snapshot`, and `clean_snapshot` are
/// test-only batch facilities. On an edit, source ownership and cheap root
/// splicing happen in begin/supersede; all scanning, resolution, emission, and
/// finalization happen in fuelled polls. Commit only adopts already-sealed
/// roots and a direct delta.
pub trait GateBEngine: Sized {
    type Error: Display;

    fn open(profile: SyntaxProfile, source: &str) -> Result<Self, Self::Error>;
    fn revision(&self) -> u64;
    fn materialize_source(&self) -> String;
    fn snapshot(&self) -> Result<Snapshot, Self::Error>;
    fn clean_snapshot(profile: SyntaxProfile, source: &str) -> Result<Snapshot, Self::Error>;
    /// Peak-live accounting at the call boundary. While work is pending this
    /// must include the old committed state, pending candidate, source roots,
    /// lexical tape, delimiter/bracket stacks, and sealed output chunks.
    fn resource_metrics(&self) -> ResourceMetrics;
    fn begin_edit(&mut self, edit: Edit) -> Result<BeginEvidence, Self::Error>;
    /// Replaces pending work using an edit against the last committed source.
    /// The old generation must become permanently uncommittable.
    fn supersede_edit(&mut self, edit: Edit) -> Result<SupersedeEvidence, Self::Error>;
    fn poll(&mut self, fuel: WorkFuel) -> Result<PollEvidence, Self::Error>;
    fn commit(&mut self) -> Result<CommitEvidence, Self::Error>;
}

fn engine_error(error: impl Display) -> String {
    error.to_string()
}

pub fn validate_delta_receipt(delta: &RevisionDelta, maximum_bytes: usize) -> Result<(), String> {
    if delta.revision != delta.base_revision + 1 {
        return Err("delta revision is not the next committed revision".to_owned());
    }
    if delta.encoded_bytes > maximum_bytes {
        return Err(format!(
            "delta is {} bytes; maximum is {maximum_bytes}",
            delta.encoded_bytes
        ));
    }
    if delta.general_batch_trees != 0 || delta.grammar_side_scans != 0 {
        return Err("delta was produced through a batch tree or grammar side scan".to_owned());
    }
    reject_duplicates(&delta.removed_leaf_ids, "removed leaf")?;
    reject_duplicates(&delta.removed_fact_ids, "removed fact")?;
    reject_duplicates(
        &delta.removed_definition_symbols,
        "removed reference definition",
    )?;
    reject_duplicates(&delta.removed_reference_use_facts, "removed reference use")?;
    reject_duplicates(
        &delta
            .upserted_leaves
            .iter()
            .map(|leaf| leaf.id)
            .collect::<Vec<_>>(),
        "upserted leaf",
    )?;
    reject_duplicates(
        &delta
            .upserted_facts
            .iter()
            .map(|fact| fact.id)
            .collect::<Vec<_>>(),
        "upserted fact",
    )?;
    reject_duplicates(
        &delta
            .upserted_definitions
            .iter()
            .map(|definition| definition.symbol)
            .collect::<Vec<_>>(),
        "upserted definition",
    )?;
    reject_duplicates(
        &delta
            .upserted_reference_uses
            .iter()
            .map(|reference_use| reference_use.fact)
            .collect::<Vec<_>>(),
        "upserted reference use",
    )?;
    if let Some(refresh) = &delta.output_sequence_refresh {
        if refresh.removed_root == refresh.inserted_root || refresh.affected_leaf_count == 0 {
            return Err(format!("invalid output sequence refresh {refresh:?}"));
        }
    }
    let mut removed_dependencies = BTreeSet::new();
    if let Some(duplicate) = delta
        .removed_label_dependencies
        .iter()
        .find(|key| !removed_dependencies.insert((*key).clone()))
    {
        return Err(format!("duplicate removed dependency {duplicate:?}"));
    }
    let mut upserted_dependencies = BTreeSet::new();
    if let Some(duplicate) = delta
        .upserted_label_dependencies
        .iter()
        .map(LabelDependency::key)
        .find(|key| !upserted_dependencies.insert(key.clone()))
    {
        return Err(format!("duplicate upserted dependency {duplicate:?}"));
    }
    let mut refreshed_labels = BTreeSet::new();
    for refresh in &delta.dependency_index_refreshes {
        if (refresh.removed_generation.is_none() && refresh.inserted_generation.is_none())
            || refresh.removed_generation == refresh.inserted_generation
            || !refreshed_labels.insert(refresh.normalized_label.clone())
        {
            return Err(format!("invalid dependency index refresh {refresh:?}"));
        }
    }
    Ok(())
}

fn reject_duplicates(ids: &[StableId], label: &str) -> Result<(), String> {
    let mut unique = BTreeSet::new();
    if let Some(duplicate) = ids.iter().find(|id| !unique.insert(**id)) {
        return Err(format!("duplicate {label} ID {duplicate:?}"));
    }
    Ok(())
}

/// Replays a direct delta over shifted unchanged records and compares every
/// stable-ID keyed output to the committed snapshot. Intersecting records must
/// be explicitly removed or upserted; a global replacement cannot hide here.
pub fn validate_replayable_delta(
    before: &Snapshot,
    after: &Snapshot,
    edit: &Edit,
    delta: &RevisionDelta,
) -> Result<(), String> {
    validate_delta_receipt(delta, 64 * 1024)?;
    if delta.base_revision != before.revision || delta.revision != after.revision {
        return Err("delta revision envelope does not match snapshots".to_owned());
    }
    if edit.base_revision != before.revision {
        return Err("edit base revision does not match before snapshot".to_owned());
    }

    let leaves = replay_records(
        &before.leaves,
        &delta.removed_leaf_ids,
        &delta.upserted_leaves,
        |leaf| leaf.id,
        |leaf| map_leaf(leaf, edit),
    )?;
    compare_id_maps(leaves, &after.leaves, |leaf| leaf.id, "leaf")?;
    validate_order_splices(
        &before.leaves.iter().map(|leaf| leaf.id).collect::<Vec<_>>(),
        &after.leaves.iter().map(|leaf| leaf.id).collect::<Vec<_>>(),
        &delta.leaf_order_splices,
        "leaf",
    )?;

    let changed_output_leaves =
        validate_output_sequence_refresh(before, after, &delta.output_sequence_refresh)?;
    if !delta.removed_fact_ids.is_empty()
        || !delta.upserted_facts.is_empty()
        || !delta.removed_reference_use_facts.is_empty()
        || !delta.upserted_reference_uses.is_empty()
    {
        return Err(
            "fact/reference changes must arrive through the immutable output root".to_owned(),
        );
    }

    let facts = replay_records_with_leaf_adoption(
        &before.facts,
        &after.facts,
        &changed_output_leaves,
        |fact| fact.id,
        |fact| fact.leaf,
        |fact| map_fact(fact, edit),
    )?;
    compare_id_maps(facts, &after.facts, |fact| fact.id, "fact")?;
    let bulk_fact_order = output_sequence_fact_order(before, after, &changed_output_leaves);
    validate_order_splices(
        &bulk_fact_order,
        &after.facts.iter().map(|fact| fact.id).collect::<Vec<_>>(),
        &delta.fact_order_splices,
        "fact",
    )?;

    let definitions = replay_records(
        &before.definitions,
        &delta.removed_definition_symbols,
        &delta.upserted_definitions,
        |definition| definition.symbol,
        |definition| map_definition(definition, edit),
    )?;
    compare_id_maps(
        definitions,
        &after.definitions,
        |definition| definition.symbol,
        "definition",
    )?;

    let before_fact_leaves = before
        .facts
        .iter()
        .map(|fact| (fact.id, fact.leaf))
        .collect::<BTreeMap<_, _>>();
    let after_fact_leaves = after
        .facts
        .iter()
        .map(|fact| (fact.id, fact.leaf))
        .collect::<BTreeMap<_, _>>();
    let uses = replay_reference_uses_with_leaf_adoption(
        &before.reference_uses,
        &after.reference_uses,
        &changed_output_leaves,
        &before_fact_leaves,
        &after_fact_leaves,
        edit,
    )?;
    compare_id_maps(
        uses,
        &after.reference_uses,
        |reference_use| reference_use.fact,
        "reference use",
    )?;
    validate_dependency_replay(before, after, delta)?;
    Ok(())
}

fn validate_output_sequence_refresh(
    before: &Snapshot,
    after: &Snapshot,
    refresh: &Option<OutputSequenceRefresh>,
) -> Result<BTreeSet<StableId>, String> {
    let before_outputs = before
        .leaf_outputs
        .iter()
        .map(|output| (output.leaf, output))
        .collect::<BTreeMap<_, _>>();
    let after_outputs = after
        .leaf_outputs
        .iter()
        .map(|output| (output.leaf, output))
        .collect::<BTreeMap<_, _>>();
    let before_positions = before
        .leaves
        .iter()
        .enumerate()
        .map(|(index, leaf)| (leaf.id, index))
        .collect::<BTreeMap<_, _>>();
    let after_positions = after
        .leaves
        .iter()
        .enumerate()
        .map(|(index, leaf)| (leaf.id, index))
        .collect::<BTreeMap<_, _>>();
    let all_leaves = before_outputs
        .keys()
        .chain(after_outputs.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let changed = all_leaves
        .into_iter()
        .filter(|leaf| {
            before_outputs.get(leaf) != after_outputs.get(leaf)
                || before_positions.get(leaf) != after_positions.get(leaf)
        })
        .collect::<BTreeSet<_>>();

    match (changed.is_empty(), refresh) {
        (true, None) if before.output_sequence_root == after.output_sequence_root => {}
        (false, Some(refresh))
            if refresh.removed_root == before.output_sequence_root
                && refresh.inserted_root == after.output_sequence_root
                && refresh.affected_leaf_count == changed.len() => {}
        (true, Some(_)) => {
            return Err("output sequence root churned without changed leaves".to_owned())
        }
        (false, None) => {
            return Err("changed leaves lack a compact output-sequence refresh".to_owned())
        }
        _ => {
            return Err("output-sequence refresh does not match committed roots/leaves".to_owned())
        }
    }
    Ok(changed)
}

fn replay_records_with_leaf_adoption<T: Clone>(
    before: &[T],
    after: &[T],
    changed_leaves: &BTreeSet<StableId>,
    id: impl Fn(&T) -> StableId,
    leaf: impl Fn(&T) -> StableId,
    map: impl Fn(&T) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    let mut replayed = before
        .iter()
        .filter(|record| !changed_leaves.contains(&leaf(record)))
        .map(map)
        .collect::<Result<Vec<_>, _>>()?;
    replayed.extend(
        after
            .iter()
            .filter(|record| changed_leaves.contains(&leaf(record)))
            .cloned(),
    );
    let mut ids = BTreeSet::new();
    if replayed.iter().any(|record| !ids.insert(id(record))) {
        return Err("bulk output adoption produced duplicate record IDs".to_owned());
    }
    Ok(replayed)
}

fn output_sequence_fact_order(
    before: &Snapshot,
    after: &Snapshot,
    changed_leaves: &BTreeSet<StableId>,
) -> Vec<StableId> {
    let mut order = Vec::new();
    for leaf in &after.leaves {
        let facts = if changed_leaves.contains(&leaf.id) {
            &after.facts
        } else {
            &before.facts
        };
        order.extend(
            facts
                .iter()
                .filter(|fact| fact.leaf == leaf.id)
                .map(|fact| fact.id),
        );
    }
    order
}

fn replay_reference_uses_with_leaf_adoption(
    before: &[ReferenceUse],
    after: &[ReferenceUse],
    changed_leaves: &BTreeSet<StableId>,
    before_fact_leaves: &BTreeMap<StableId, StableId>,
    after_fact_leaves: &BTreeMap<StableId, StableId>,
    edit: &Edit,
) -> Result<Vec<ReferenceUse>, String> {
    let mut replayed = before
        .iter()
        .filter(|reference_use| {
            before_fact_leaves
                .get(&reference_use.fact)
                .is_none_or(|leaf| !changed_leaves.contains(leaf))
        })
        .map(|reference_use| map_reference_use(reference_use, edit))
        .collect::<Result<Vec<_>, _>>()?;
    replayed.extend(
        after
            .iter()
            .filter(|reference_use| {
                after_fact_leaves
                    .get(&reference_use.fact)
                    .is_some_and(|leaf| changed_leaves.contains(leaf))
            })
            .cloned(),
    );
    Ok(replayed)
}

fn validate_dependency_replay(
    before: &Snapshot,
    after: &Snapshot,
    delta: &RevisionDelta,
) -> Result<(), String> {
    let refreshed_labels = delta
        .dependency_index_refreshes
        .iter()
        .map(|refresh| refresh.normalized_label.as_str())
        .collect::<BTreeSet<_>>();
    let before_indexes = before
        .dependency_indexes
        .iter()
        .map(|index| (index.normalized_label.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let after_indexes = after
        .dependency_indexes
        .iter()
        .map(|index| (index.normalized_label.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    for refresh in &delta.dependency_index_refreshes {
        let before_generation = before_indexes
            .get(refresh.normalized_label.as_str())
            .map(|index| index.generation);
        let after_generation = after_indexes
            .get(refresh.normalized_label.as_str())
            .map(|index| index.generation);
        if before_generation != refresh.removed_generation
            || after_generation != refresh.inserted_generation
        {
            return Err("dependency refresh generation does not match snapshot".to_owned());
        }
    }
    let all_labels = before_indexes
        .keys()
        .chain(after_indexes.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    for label in all_labels {
        let changed = before_indexes.get(label) != after_indexes.get(label);
        if changed != refreshed_labels.contains(label) {
            return Err(format!(
                "dependency index change for {label:?} lacks/exceeds refresh"
            ));
        }
    }

    if delta
        .removed_label_dependencies
        .iter()
        .any(|key| refreshed_labels.contains(key.normalized_label.as_str()))
        || delta
            .upserted_label_dependencies
            .iter()
            .any(|dependency| refreshed_labels.contains(dependency.normalized_label.as_str()))
    {
        return Err("bulk dependency refresh also enumerated per-leaf dependencies".to_owned());
    }
    let removed = delta
        .removed_label_dependencies
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let upserted = delta
        .upserted_label_dependencies
        .iter()
        .map(|dependency| (dependency.key(), dependency.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut replayed = before
        .label_dependencies
        .iter()
        .filter(|dependency| {
            !refreshed_labels.contains(dependency.normalized_label.as_str())
                && !removed.contains(&dependency.key())
                && !upserted.contains_key(&dependency.key())
        })
        .cloned()
        .collect::<Vec<_>>();
    replayed.extend(delta.upserted_label_dependencies.iter().cloned());
    replayed.extend(
        after
            .label_dependencies
            .iter()
            .filter(|dependency| refreshed_labels.contains(dependency.normalized_label.as_str()))
            .cloned(),
    );
    let replayed = replayed
        .into_iter()
        .map(|dependency| (dependency.key(), dependency))
        .collect::<BTreeMap<_, _>>();
    let committed = after
        .label_dependencies
        .iter()
        .cloned()
        .map(|dependency| (dependency.key(), dependency))
        .collect::<BTreeMap<_, _>>();
    if replayed != committed {
        return Err("dependency delta does not replay to committed state".to_owned());
    }
    Ok(())
}

fn validate_order_splices(
    before: &[StableId],
    after: &[StableId],
    splices: &[OrderSplice],
    label: &str,
) -> Result<(), String> {
    let mut replayed = before.to_vec();
    for splice in splices {
        let end = splice
            .start
            .checked_add(splice.removed.len())
            .ok_or_else(|| format!("{label} order splice overflow"))?;
        if end > replayed.len() || replayed[splice.start..end] != splice.removed {
            return Err(format!(
                "{label} order splice does not match the replay sequence"
            ));
        }
        replayed.splice(splice.start..end, splice.inserted.iter().copied());
    }
    if replayed != after {
        return Err(format!(
            "{label} order splices do not replay to committed order"
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectedRange {
    pub old: Range<usize>,
}

/// Stronger identity check for edits with explicitly bounded semantic blast
/// radii. Records fully contained in protected prefix/suffix regions must keep
/// IDs and must not be redundantly upserted.
pub fn validate_protected_identity(
    before: &Snapshot,
    after: &Snapshot,
    edit: &Edit,
    delta: &RevisionDelta,
    protected: &[ProtectedRange],
) -> Result<(), String> {
    let after_leaves = before_after_map(&after.leaves, |leaf| leaf.id);
    let after_facts = before_after_map(&after.facts, |fact| fact.id);
    let after_definitions = before_after_map(&after.definitions, |definition| definition.symbol);
    let after_uses = before_after_map(&after.reference_uses, |reference_use| reference_use.fact);
    let before_outputs = before
        .leaf_outputs
        .iter()
        .map(|output| (output.leaf, output))
        .collect::<BTreeMap<_, _>>();
    let after_outputs = after
        .leaf_outputs
        .iter()
        .map(|output| (output.leaf, output))
        .collect::<BTreeMap<_, _>>();
    let removed_leaves = delta
        .removed_leaf_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let upserted_leaves = delta
        .upserted_leaves
        .iter()
        .map(|leaf| leaf.id)
        .collect::<BTreeSet<_>>();
    let leaf_order_touched = delta
        .leaf_order_splices
        .iter()
        .flat_map(|splice| splice.removed.iter().chain(&splice.inserted))
        .copied()
        .collect::<BTreeSet<_>>();
    let removed_facts = delta
        .removed_fact_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let upserted_facts = delta
        .upserted_facts
        .iter()
        .map(|fact| fact.id)
        .collect::<BTreeSet<_>>();
    let fact_order_touched = delta
        .fact_order_splices
        .iter()
        .flat_map(|splice| splice.removed.iter().chain(&splice.inserted))
        .copied()
        .collect::<BTreeSet<_>>();
    let removed_definitions = delta
        .removed_definition_symbols
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let upserted_definitions = delta
        .upserted_definitions
        .iter()
        .map(|definition| definition.symbol)
        .collect::<BTreeSet<_>>();
    let removed_uses = delta
        .removed_reference_use_facts
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let upserted_uses = delta
        .upserted_reference_uses
        .iter()
        .map(|reference_use| reference_use.fact)
        .collect::<BTreeSet<_>>();

    for protected_range in protected {
        let protected_record_count = before
            .leaves
            .iter()
            .filter(|leaf| contains(&protected_range.old, &leaf.source))
            .count()
            + before
                .facts
                .iter()
                .filter(|fact| contains(&protected_range.old, &fact.source))
                .count()
            + before
                .definitions
                .iter()
                .filter(|definition| contains(&protected_range.old, &definition.source))
                .count()
            + before
                .reference_uses
                .iter()
                .filter(|reference_use| contains(&protected_range.old, &reference_use.label_source))
                .count();
        if protected_record_count == 0 {
            return Err(format!(
                "protected range {:?} has no independently stable record",
                protected_range.old
            ));
        }
        for leaf in before
            .leaves
            .iter()
            .filter(|leaf| contains(&protected_range.old, &leaf.source))
        {
            let mapped = map_leaf(leaf, edit)?;
            if after_leaves.get(&leaf.id) != Some(&&mapped)
                || removed_leaves.contains(&leaf.id)
                || upserted_leaves.contains(&leaf.id)
                || leaf_order_touched.contains(&leaf.id)
            {
                return Err(format!("protected leaf ID {:?} churned", leaf.id));
            }
            if before_outputs.get(&leaf.id) != after_outputs.get(&leaf.id) {
                return Err(format!("protected leaf output root {:?} churned", leaf.id));
            }
        }
        for fact in before
            .facts
            .iter()
            .filter(|fact| contains(&protected_range.old, &fact.source))
        {
            let mapped = map_fact(fact, edit)?;
            if after_facts.get(&fact.id) != Some(&&mapped)
                || removed_facts.contains(&fact.id)
                || upserted_facts.contains(&fact.id)
                || fact_order_touched.contains(&fact.id)
            {
                return Err(format!("protected fact ID {:?} churned", fact.id));
            }
        }
        for definition in before
            .definitions
            .iter()
            .filter(|definition| contains(&protected_range.old, &definition.source))
        {
            let mapped = map_definition(definition, edit)?;
            if after_definitions.get(&definition.symbol) != Some(&&mapped)
                || removed_definitions.contains(&definition.symbol)
                || upserted_definitions.contains(&definition.symbol)
            {
                return Err(format!(
                    "protected definition symbol {:?} churned",
                    definition.symbol
                ));
            }
        }
        for reference_use in before
            .reference_uses
            .iter()
            .filter(|reference_use| contains(&protected_range.old, &reference_use.label_source))
        {
            let mapped = map_reference_use(reference_use, edit)?;
            if after_uses.get(&reference_use.fact) != Some(&&mapped)
                || removed_uses.contains(&reference_use.fact)
                || upserted_uses.contains(&reference_use.fact)
            {
                return Err(format!(
                    "protected reference use {:?} churned",
                    reference_use.fact
                ));
            }
        }
    }
    Ok(())
}

fn before_after_map<T>(records: &[T], id: impl Fn(&T) -> StableId) -> BTreeMap<StableId, &T> {
    records.iter().map(|record| (id(record), record)).collect()
}

fn contains(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

fn replay_records<T: Clone>(
    before: &[T],
    removed: &[StableId],
    upserted: &[T],
    id: impl Fn(&T) -> StableId,
    map: impl Fn(&T) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    let removed = removed.iter().copied().collect::<BTreeSet<_>>();
    let upserted_ids = upserted.iter().map(&id).collect::<BTreeSet<_>>();
    let mut replayed = before
        .iter()
        .filter(|record| !removed.contains(&id(record)) && !upserted_ids.contains(&id(record)))
        .map(map)
        .collect::<Result<Vec<_>, _>>()?;
    replayed.extend_from_slice(upserted);
    Ok(replayed)
}

fn compare_id_maps<T: Clone + PartialEq + std::fmt::Debug>(
    replayed: Vec<T>,
    committed: &[T],
    id: impl Fn(&T) -> StableId,
    label: &str,
) -> Result<(), String> {
    let replayed = replayed
        .into_iter()
        .map(|record| (id(&record), record))
        .collect::<BTreeMap<_, _>>();
    let committed = committed
        .iter()
        .cloned()
        .map(|record| (id(&record), record))
        .collect::<BTreeMap<_, _>>();
    if replayed != committed {
        return Err(format!("{label} delta does not replay to committed state"));
    }
    Ok(())
}

fn map_leaf(leaf: &InlineLeaf, edit: &Edit) -> Result<InlineLeaf, String> {
    Ok(InlineLeaf {
        id: leaf.id,
        source: map_range(&leaf.source, edit)?,
        input: map_segmented(&leaf.input, edit)?,
    })
}

fn map_fact(fact: &InlineFact, edit: &Edit) -> Result<InlineFact, String> {
    Ok(InlineFact {
        id: fact.id,
        leaf: fact.leaf,
        parent: fact.parent,
        kind: fact.kind.clone(),
        source: map_range(&fact.source, edit)?,
        markers: fact
            .markers
            .iter()
            .map(|range| map_range(range, edit))
            .collect::<Result<Vec<_>, _>>()?,
        content: map_segmented(&fact.content, edit)?,
    })
}

fn map_definition(
    definition: &ReferenceDefinition,
    edit: &Edit,
) -> Result<ReferenceDefinition, String> {
    Ok(ReferenceDefinition {
        symbol: definition.symbol,
        normalized_label: definition.normalized_label.clone(),
        destination: definition.destination.clone(),
        title: definition.title.clone(),
        source: map_range(&definition.source, edit)?,
        markers: definition
            .markers
            .iter()
            .map(|range| map_range(range, edit))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn map_reference_use(reference_use: &ReferenceUse, edit: &Edit) -> Result<ReferenceUse, String> {
    Ok(ReferenceUse {
        fact: reference_use.fact,
        symbol: reference_use.symbol,
        label_source: map_range(&reference_use.label_source, edit)?,
    })
}

fn map_segmented(mapped: &SegmentedText, edit: &Edit) -> Result<SegmentedText, String> {
    Ok(SegmentedText {
        text: mapped.text.clone(),
        pieces: mapped
            .pieces
            .iter()
            .map(|piece| match piece {
                MapPiece::Source(range) => Ok(MapPiece::Source(map_range(range, edit)?)),
                MapPiece::Virtual {
                    anchor_utf8,
                    text,
                    reason,
                } => Ok(MapPiece::Virtual {
                    anchor_utf8: map_anchor(*anchor_utf8, edit)?,
                    text: text.clone(),
                    reason: *reason,
                }),
            })
            .collect::<Result<Vec<_>, String>>()?,
    })
}

fn map_anchor(anchor: usize, edit: &Edit) -> Result<usize, String> {
    if anchor <= edit.start_utf8 {
        return Ok(anchor);
    }
    if edit.end_utf8 <= anchor {
        return anchor
            .checked_add_signed(
                edit.replacement.len() as isize - (edit.end_utf8 - edit.start_utf8) as isize,
            )
            .ok_or_else(|| "virtual anchor mapping underflow".to_owned());
    }
    Err("virtual anchor intersects edit".to_owned())
}

fn map_range(range: &Range<usize>, edit: &Edit) -> Result<Range<usize>, String> {
    if range.end <= edit.start_utf8 {
        return Ok(range.clone());
    }
    if edit.end_utf8 <= range.start {
        let delta = edit.replacement.len() as isize - (edit.end_utf8 - edit.start_utf8) as isize;
        return Ok(range
            .start
            .checked_add_signed(delta)
            .ok_or_else(|| "range mapping underflow".to_owned())?
            ..range
                .end
                .checked_add_signed(delta)
                .ok_or_else(|| "range mapping underflow".to_owned())?);
    }
    Err(format!("range {range:?} intersects edit without upsert"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactProbe {
    pub tag: FactTag,
    pub source_text: String,
    pub marker_texts: Vec<String>,
    pub content_text: Option<String>,
    pub parent_tag: Option<FactTag>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafProbe {
    pub input_text: Option<String>,
    pub source_fragments: Vec<String>,
    pub virtual_reasons: Vec<VirtualReason>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionExpectation {
    pub source: String,
    pub fact_probes: Vec<FactProbe>,
    pub leaf_probes: Vec<LeafProbe>,
}

impl RevisionExpectation {
    pub fn source(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            fact_probes: Vec::new(),
            leaf_probes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditHistory {
    pub name: &'static str,
    pub profile: SyntaxProfile,
    pub revisions: Vec<RevisionExpectation>,
}

pub fn gate_b_histories() -> Vec<EditHistory> {
    let mut histories = vec![
        typing_and_erasing_history(
            "nested-delimiters-every-revision",
            "**alpha _beta_** and ~~gamma~~\n",
        ),
        typing_and_erasing_history(
            "code-spans-every-revision",
            "`one` `` two ` ticks `` and `a\tb`\n",
        ),
        typing_and_erasing_history(
            "links-images-every-revision",
            "[text](https://e.test \"title\") ![alt][image]\n\n[image]: /i.png\n",
        ),
        typing_and_erasing_history(
            "escapes-entities-breaks-every-revision",
            "\\*literal* &amp; &#x41; hard\\\nsoft\n",
        ),
        typing_and_erasing_history(
            "gfm-autolink-strike-every-revision",
            "~~gone~~ <https://e.test> www.example.com/a_(b) and me@example.com\n",
        ),
        typing_and_erasing_history(
            "escaped-and-code-table-pipes-every-revision",
            "| escaped | code |\n| --- | --- |\n| a\\|b | `c\\|d` |\n",
        ),
        typing_and_erasing_history(
            "quote-list-segmented-inline-every-revision",
            "> - **alpha**\n>   beta and `gamma`\n",
        ),
        typing_and_erasing_history(
            "quote-list-tab-virtualization-every-revision",
            "> - alpha\tbeta\n>   gamma\n",
        ),
        explicit_history(
            "reference-resolution-toggle",
            &[
                "[label] [label]\n\n[label]: /one\n",
                "[label] [label]\n\n[label]: /two\n",
                "[label] [label]\n\n[other]: /two\n",
                "[label] [label]\n\n[label]: /one\n",
            ],
        ),
        explicit_history(
            "shared-label-normalization-task-marker-seam",
            &[
                "- [x] [Foo\t Bar]\n\n[foo bar]: /target\n",
                "- [ ] [FOO  BAR]\n\n[foo bar]: /target\n",
                "- [x] [foo bar]\n\n[^foo bar]: /footnote\n",
                "- [x] [foo bar]\n\n[foo bar]: /target\n",
            ],
        ),
        explicit_history(
            "flark-footnote-alert-raw-html-policy",
            &[
                "[^1]\n\n[^1]: note\n",
                "> [!NOTE]\n> body\n",
                "before <script>alert(1)</script> after\n",
                "[^1]\n\n[^1]: note\n",
            ],
        ),
    ];

    let nested_final = histories[0].revisions.len() / 2;
    histories[0].revisions[nested_final].fact_probes = vec![
        fact_probe(FactTag::Strong, "**alpha _beta_**", &["**", "**"]),
        FactProbe {
            parent_tag: Some(FactTag::Strong),
            ..fact_probe(FactTag::Emphasis, "_beta_", &["_", "_"])
        },
        fact_probe(FactTag::Strikethrough, "~~gamma~~", &["~~", "~~"]),
    ];
    let code_final = histories[1].revisions.len() / 2;
    histories[1].revisions[code_final].fact_probes = vec![
        fact_probe(FactTag::CodeSpan, "`one`", &["`", "`"]),
        fact_probe(FactTag::CodeSpan, "`` two ` ticks ``", &["``", "``"]),
    ];
    let links_final = histories[2].revisions.len() / 2;
    histories[2].revisions[links_final].fact_probes = vec![
        fact_probe(
            FactTag::Link,
            "[text](https://e.test \"title\")",
            &["[", "](https://e.test \"title\")"],
        ),
        fact_probe(FactTag::Image, "![alt][image]", &["![", "][image]"]),
    ];
    let autolink_final = histories[4].revisions.len() / 2;
    histories[4].revisions[autolink_final].fact_probes = vec![
        fact_probe(FactTag::Strikethrough, "~~gone~~", &["~~", "~~"]),
        fact_probe(FactTag::Autolink, "<https://e.test>", &["<", ">"]),
        fact_probe(FactTag::Autolink, "www.example.com/a_(b)", &[]),
        fact_probe(FactTag::Autolink, "me@example.com", &[]),
    ];
    let segmented_final = histories[6].revisions.len() / 2;
    histories[6].revisions[segmented_final]
        .leaf_probes
        .push(LeafProbe {
            input_text: Some("**alpha**\nbeta and `gamma`".to_owned()),
            source_fragments: vec!["**alpha**".to_owned(), "beta and `gamma`".to_owned()],
            virtual_reasons: vec![VirtualReason::ContainerLineJoin],
        });
    let tab_final = histories[7].revisions.len() / 2;
    histories[7].revisions[tab_final]
        .leaf_probes
        .push(LeafProbe {
            input_text: None,
            source_fragments: vec!["alpha".to_owned(), "beta".to_owned(), "gamma".to_owned()],
            virtual_reasons: vec![
                VirtualReason::TabExpansion,
                VirtualReason::ContainerLineJoin,
            ],
        });
    let table_final = histories[5].revisions.len() / 2;
    histories[5].revisions[table_final].fact_probes = vec![
        FactProbe {
            tag: FactTag::Text,
            source_text: "\\|".to_owned(),
            marker_texts: vec!["\\".to_owned()],
            content_text: Some("|".to_owned()),
            parent_tag: None,
        },
        FactProbe {
            tag: FactTag::CodeSpan,
            source_text: "`c\\|d`".to_owned(),
            marker_texts: vec!["`".to_owned(), "`".to_owned()],
            content_text: Some("c|d".to_owned()),
            parent_tag: None,
        },
    ];
    histories[9].revisions[0].fact_probes.push(fact_probe(
        FactTag::Link,
        "[Foo\t Bar]",
        &["[", "]"],
    ));
    histories[10].revisions[2].fact_probes.push(fact_probe(
        FactTag::RawHtml,
        "<script>",
        &["<", ">"],
    ));
    histories
}

fn fact_probe(tag: FactTag, source_text: &str, marker_texts: &[&str]) -> FactProbe {
    FactProbe {
        tag,
        source_text: source_text.to_owned(),
        marker_texts: marker_texts
            .iter()
            .map(|marker| (*marker).to_owned())
            .collect(),
        content_text: None,
        parent_tag: None,
    }
}

fn explicit_history(name: &'static str, sources: &[&str]) -> EditHistory {
    EditHistory {
        name,
        profile: SyntaxProfile::FlarkGfm,
        revisions: sources
            .iter()
            .map(|source| RevisionExpectation::source(*source))
            .collect(),
    }
}

fn typing_and_erasing_history(name: &'static str, final_source: &str) -> EditHistory {
    let mut revisions = vec![RevisionExpectation::source("")];
    let mut boundaries = final_source
        .char_indices()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    boundaries.push(final_source.len());
    for boundary in boundaries.iter().copied().skip(1) {
        revisions.push(RevisionExpectation::source(&final_source[..boundary]));
    }
    for boundary in boundaries.iter().copied().rev().skip(1) {
        revisions.push(RevisionExpectation::source(&final_source[..boundary]));
    }
    EditHistory {
        name,
        profile: SyntaxProfile::FlarkGfm,
        revisions,
    }
}

pub fn validate_fact_probes(
    source: &str,
    snapshot: &Snapshot,
    probes: &[FactProbe],
) -> Result<(), String> {
    for probe in probes {
        let by_id = snapshot
            .facts
            .iter()
            .map(|fact| (fact.id, fact))
            .collect::<BTreeMap<_, _>>();
        let matching = snapshot.facts.iter().find(|fact| {
            fact.kind.tag() == probe.tag
                && source.get(fact.source.clone()) == Some(probe.source_text.as_str())
                && fact
                    .markers
                    .iter()
                    .map(|range| source.get(range.clone()).unwrap_or(""))
                    .eq(probe.marker_texts.iter().map(String::as_str))
                && probe
                    .content_text
                    .as_ref()
                    .is_none_or(|text| fact.content.text == *text)
                && probe.parent_tag.is_none_or(|parent_tag| {
                    fact.parent
                        .and_then(|parent| by_id.get(&parent).copied())
                        .is_some_and(|parent| parent.kind.tag() == parent_tag)
                })
        });
        if matching.is_none() {
            return Err(format!("missing exact fact probe {probe:?}"));
        }
    }
    Ok(())
}

pub fn validate_leaf_probes(
    source: &str,
    snapshot: &Snapshot,
    probes: &[LeafProbe],
) -> Result<(), String> {
    for probe in probes {
        let matching = snapshot.leaves.iter().find(|leaf| {
            if probe
                .input_text
                .as_ref()
                .is_some_and(|input| leaf.input.text != *input)
            {
                return false;
            }
            let source_fragments = leaf.input.pieces.iter().filter_map(|piece| match piece {
                MapPiece::Source(range) => source.get(range.clone()).map(str::to_owned),
                MapPiece::Virtual { .. } => None,
            });
            let reasons = leaf.input.pieces.iter().filter_map(|piece| match piece {
                MapPiece::Virtual { reason, .. } => Some(*reason),
                MapPiece::Source(_) => None,
            });
            source_fragments.eq(probe.source_fragments.iter().cloned())
                && reasons.eq(probe.virtual_reasons.iter().copied())
        });
        if matching.is_none() {
            return Err(format!("missing exact segmented leaf probe {probe:?}"));
        }
    }
    Ok(())
}

/// Enforces Flark-specific exclusions that cannot be represented by upstream
/// positive fixtures alone.
pub fn validate_flark_profile(source: &str, snapshot: &Snapshot) -> Result<(), String> {
    if source.contains("[^1]")
        && (snapshot
            .definitions
            .iter()
            .any(|definition| definition.normalized_label.starts_with('^'))
            || snapshot.reference_uses.iter().any(|reference_use| {
                snapshot
                    .definitions
                    .iter()
                    .find(|definition| definition.symbol == reference_use.symbol)
                    .is_some_and(|definition| definition.normalized_label.starts_with('^'))
            }))
    {
        return Err("footnote-shaped source became a reference".to_owned());
    }
    if source.contains("[!NOTE]")
        && snapshot.facts.iter().any(|fact| {
            matches!(
                fact.kind,
                InlineKind::Link { .. } | InlineKind::Image { .. }
            ) && source
                .get(fact.source.clone())
                .is_some_and(|text| text.contains("[!NOTE]"))
        })
    {
        return Err("GitHub alert label became link syntax".to_owned());
    }
    if source.contains("<script>")
        && !snapshot.facts.iter().any(|fact| {
            matches!(
                fact.kind,
                InlineKind::RawHtml {
                    display: DisplayPolicy::SourceVisible
                }
            ) && source.get(fact.source.clone()) == Some("<script>")
        })
    {
        return Err("raw HTML lacks a source-visible syntax fact".to_owned());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceCaps {
    pub total_peak_live_bytes: usize,
    pub source_backing_bytes: usize,
    pub persistent_auxiliary_bytes: usize,
    pub inline_output_bytes: usize,
    pub source_map_bytes: usize,
    pub reference_index_bytes: usize,
    pub lexical_tape_bytes: usize,
    pub resolution_stack_bytes: usize,
    pub retained_source_prefix_bytes: usize,
    pub peak_scratch_bytes: usize,
}

impl ResourceCaps {
    /// A 96 MiB kill ceiling around a roughly 58 MiB target envelope. The
    /// lexical tape cap specifically rejects 12-16 byte/event representations
    /// on a 10 MiB alternating-special-byte leaf.
    pub const TOKEN_DENSE: Self = Self {
        total_peak_live_bytes: 96 * 1024 * 1024,
        source_backing_bytes: 24 * 1024 * 1024,
        persistent_auxiliary_bytes: 64 * 1024 * 1024,
        inline_output_bytes: 48 * 1024 * 1024,
        source_map_bytes: 16 * 1024 * 1024,
        reference_index_bytes: 8 * 1024 * 1024,
        lexical_tape_bytes: 24 * 1024 * 1024,
        resolution_stack_bytes: 16 * 1024 * 1024,
        retained_source_prefix_bytes: 64 * 1024,
        peak_scratch_bytes: 8 * 1024 * 1024,
    };

    /// Plain data and soft breaks must remain packed; syntax-node density is
    /// not proportional to byte or line count here.
    pub const PLAIN: Self = Self {
        total_peak_live_bytes: 30 * 1024 * 1024,
        source_backing_bytes: 6 * 1024 * 1024,
        persistent_auxiliary_bytes: 16 * 1024 * 1024,
        inline_output_bytes: 6 * 1024 * 1024,
        source_map_bytes: 6 * 1024 * 1024,
        reference_index_bytes: 1024 * 1024,
        lexical_tape_bytes: 4 * 1024 * 1024,
        resolution_stack_bytes: 2 * 1024 * 1024,
        retained_source_prefix_bytes: 64 * 1024,
        peak_scratch_bytes: 8 * 1024 * 1024,
    };

    pub const REFERENCE_FANOUT: Self = Self {
        total_peak_live_bytes: 36 * 1024 * 1024,
        source_backing_bytes: 4 * 1024 * 1024,
        persistent_auxiliary_bytes: 24 * 1024 * 1024,
        inline_output_bytes: 16 * 1024 * 1024,
        source_map_bytes: 8 * 1024 * 1024,
        reference_index_bytes: 4 * 1024 * 1024,
        lexical_tape_bytes: 4 * 1024 * 1024,
        resolution_stack_bytes: 4 * 1024 * 1024,
        retained_source_prefix_bytes: 64 * 1024,
        peak_scratch_bytes: 8 * 1024 * 1024,
    };
}

pub fn validate_resources(metrics: &ResourceMetrics, caps: ResourceCaps) -> Result<(), String> {
    if metrics.total_peak_live_bytes != metrics.accounted_peak_live_bytes() {
        return Err(format!(
            "total peak live {} does not equal categorized accounting {}",
            metrics.total_peak_live_bytes,
            metrics.accounted_peak_live_bytes()
        ));
    }
    if metrics
        .committed_output_bytes
        .saturating_add(metrics.pending_output_bytes)
        != metrics.categorized_output_bytes()
    {
        return Err("committed plus pending output does not equal output categories".to_owned());
    }
    if metrics.total_peak_live_bytes > caps.total_peak_live_bytes {
        return Err(format!(
            "total peak live {} exceeds {} bytes",
            metrics.total_peak_live_bytes, caps.total_peak_live_bytes
        ));
    }
    if metrics.source_backing_bytes > caps.source_backing_bytes {
        return Err(format!(
            "committed plus pending source backing {} exceeds {} bytes",
            metrics.source_backing_bytes, caps.source_backing_bytes
        ));
    }
    if metrics.persistent_auxiliary_bytes() > caps.persistent_auxiliary_bytes {
        return Err(format!(
            "persistent auxiliary state {} exceeds {} bytes",
            metrics.persistent_auxiliary_bytes(),
            caps.persistent_auxiliary_bytes
        ));
    }
    if metrics.inline_output_bytes > caps.inline_output_bytes {
        return Err("inline output memory exceeds cap".to_owned());
    }
    if metrics.source_map_bytes > caps.source_map_bytes {
        return Err("segmented source-map memory exceeds cap".to_owned());
    }
    if metrics.reference_index_bytes > caps.reference_index_bytes {
        return Err("reference index memory exceeds cap".to_owned());
    }
    if metrics.lexical_tape_bytes > caps.lexical_tape_bytes {
        return Err("lexical tape memory exceeds cap".to_owned());
    }
    if metrics.resolution_stack_bytes > caps.resolution_stack_bytes {
        return Err("delimiter/bracket resolution stack memory exceeds cap".to_owned());
    }
    if metrics.retained_source_prefix_bytes > caps.retained_source_prefix_bytes {
        return Err("resumable state retained an unbounded source prefix".to_owned());
    }
    if metrics.peak_scratch_bytes > caps.peak_scratch_bytes {
        return Err("peak scratch memory exceeds cap".to_owned());
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryReport {
    pub revisions: usize,
    pub polls: usize,
    pub pending_polls: usize,
    pub maximum_delta_bytes: usize,
}

pub fn run_history<E: GateBEngine>(
    history: &EditHistory,
    caps: ResourceCaps,
) -> Result<HistoryReport, String> {
    let first = history
        .revisions
        .first()
        .ok_or_else(|| format!("{} has no revisions", history.name))?;
    let mut engine = E::open(history.profile, &first.source).map_err(engine_error)?;
    let mut before = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&first.source, &before)?;
    validate_fact_probes(&first.source, &before, &first.fact_probes)?;
    validate_leaf_probes(&first.source, &before, &first.leaf_probes)?;
    validate_flark_profile(&first.source, &before)?;
    if history.name == "shared-label-normalization-task-marker-seam"
        && first.source.contains("[foo bar]: /target")
    {
        validate_label_task_seam(&first.source, &before)?;
    }
    validate_resources(&before.resources, caps)?;
    let clean = E::clean_snapshot(history.profile, &first.source).map_err(engine_error)?;
    validate_clean_equivalence(&before, &clean)?;

    let mut report = HistoryReport {
        revisions: 1,
        ..HistoryReport::default()
    };
    let mut previous = first.source.clone();
    for expected in history.revisions.iter().skip(1) {
        let edit = minimal_edit(engine.revision(), &previous, &expected.source);
        if edit.apply(&previous)? != expected.source {
            return Err(format!("{} generated an invalid edit", history.name));
        }
        let begin = engine.begin_edit(edit.clone()).map_err(engine_error)?;
        validate_phase_evidence(&begin.phase, "begin_edit")?;
        if begin.token.revision != before.revision + 1 {
            return Err("begin_edit returned the wrong candidate revision".to_owned());
        }
        loop {
            let poll = engine.poll(WorkFuel::GATE_B).map_err(engine_error)?;
            validate_poll_evidence(&poll, WorkFuel::GATE_B, begin.token)?;
            validate_resources(&engine.resource_metrics(), caps)?;
            report.polls += 1;
            match poll.receipt.status {
                PollStatus::Pending => report.pending_polls += 1,
                PollStatus::Ready => break,
                PollStatus::Cancelled => {
                    return Err("ordinary history unexpectedly cancelled".to_owned())
                }
            }
            if report.polls > 20_000_000 {
                return Err("history exceeded poll safety bound".to_owned());
            }
        }
        let commit = engine.commit().map_err(engine_error)?;
        validate_phase_evidence(&commit.phase, "commit")?;
        if commit.token != begin.token {
            return Err("commit adopted a different pending generation".to_owned());
        }
        if engine.materialize_source() != expected.source {
            return Err("committed source differs from requested source".to_owned());
        }
        let after = engine.snapshot().map_err(engine_error)?;
        validate_snapshot(&expected.source, &after)?;
        validate_fact_probes(&expected.source, &after, &expected.fact_probes)?;
        validate_leaf_probes(&expected.source, &after, &expected.leaf_probes)?;
        validate_flark_profile(&expected.source, &after)?;
        if history.name == "shared-label-normalization-task-marker-seam"
            && expected.source.contains("[foo bar]: /target")
        {
            validate_label_task_seam(&expected.source, &after)?;
        }
        validate_resources(&after.resources, caps)?;
        let clean = E::clean_snapshot(history.profile, &expected.source).map_err(engine_error)?;
        validate_clean_equivalence(&after, &clean)?;
        validate_replayable_delta(&before, &after, &edit, &commit.delta)?;
        report.maximum_delta_bytes = report.maximum_delta_bytes.max(commit.delta.encoded_bytes);
        report.revisions += 1;
        previous = expected.source.clone();
        before = after;
    }
    Ok(report)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StressEditCase {
    pub name: &'static str,
    pub source: String,
    pub first_edit: Edit,
    pub superseding_edit: Edit,
    pub caps: ResourceCaps,
    pub require_pending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalityCase {
    pub source: String,
    pub edit: Edit,
    pub protected: Vec<ProtectedRange>,
}

pub fn inline_locality_case() -> LocalityCase {
    let source = "**prefix**\n\nmiddle *value*\n\n[suffix](https://e.test)\n".to_owned();
    let value = source
        .find("value")
        .expect("literal fixture contains value");
    let suffix = source
        .find("[suffix]")
        .expect("literal fixture contains suffix");
    LocalityCase {
        edit: Edit {
            base_revision: 0,
            start_utf8: value,
            end_utf8: value + "value".len(),
            replacement: "other".to_owned(),
        },
        protected: vec![
            ProtectedRange { old: 0..12 },
            ProtectedRange {
                old: suffix..source.len(),
            },
        ],
        source,
    }
}

pub fn token_dense_leaf(minimum_bytes: usize) -> String {
    let pattern = "***a*** ~~b~~ [c](https://e.test) `d` &amp; \\* ";
    let target = minimum_bytes.max(3);
    let mut source = String::with_capacity(target + pattern.len());
    // The outer code delimiter makes changing byte zero globally reclassify a
    // physically token-dense leaf; all newly active delimiters must then be
    // scanned, resolved, emitted, and finalized under poll fuel.
    source.push('`');
    while source.len() + pattern.len() + 2 <= target {
        source.push_str(pattern);
    }
    while source.len() + 2 < target {
        source.push('a');
    }
    source.push('`');
    source.push('\n');
    source
}

pub fn delimiter_run_source(minimum_bytes: usize) -> String {
    let mut source = "*".repeat(minimum_bytes.max(1));
    source.push_str(" a\n");
    source
}

pub fn alternating_special_source(minimum_bytes: usize) -> String {
    let pattern = "*_[`~<\\&]";
    let target = minimum_bytes.max(3);
    let mut source = String::with_capacity(target + pattern.len());
    source.push('`');
    while source.len() + pattern.len() + 2 <= target {
        source.push_str(pattern);
    }
    while source.len() + 2 < target {
        source.push('*');
    }
    source.push('`');
    source.push('\n');
    source
}

pub fn giant_inline_cases() -> Vec<StressEditCase> {
    let token_dense = token_dense_leaf(10 * 1024 * 1024);
    let delimiter_run = delimiter_run_source(10 * 1024 * 1024);
    let alternating = alternating_special_source(10 * 1024 * 1024);
    vec![
        StressEditCase {
            name: "10mb-token-dense-global-code-toggle",
            source: token_dense,
            first_edit: Edit {
                base_revision: 0,
                start_utf8: 0,
                end_utf8: 1,
                replacement: "~".to_owned(),
            },
            superseding_edit: Edit {
                base_revision: 0,
                start_utf8: 0,
                end_utf8: 1,
                replacement: "_".to_owned(),
            },
            caps: ResourceCaps::TOKEN_DENSE,
            require_pending: true,
        },
        StressEditCase {
            name: "10mb-single-delimiter-run",
            source: delimiter_run,
            first_edit: Edit {
                base_revision: 0,
                start_utf8: 0,
                end_utf8: 1,
                replacement: "_".to_owned(),
            },
            superseding_edit: Edit {
                base_revision: 0,
                start_utf8: 0,
                end_utf8: 1,
                replacement: "~".to_owned(),
            },
            caps: ResourceCaps::TOKEN_DENSE,
            require_pending: true,
        },
        StressEditCase {
            name: "10mb-alternating-special-byte-leaf",
            source: alternating,
            first_edit: Edit {
                base_revision: 0,
                start_utf8: 0,
                end_utf8: 1,
                replacement: "~".to_owned(),
            },
            superseding_edit: Edit {
                base_revision: 0,
                start_utf8: 0,
                end_utf8: 1,
                replacement: "_".to_owned(),
            },
            caps: ResourceCaps::TOKEN_DENSE,
            require_pending: true,
        },
    ]
}

pub fn plain_softbreak_source(lines: usize) -> String {
    "a\n".repeat(lines)
}

pub fn validate_plain_run_coalescing(snapshot: &Snapshot) -> Result<(), String> {
    let maximum_runs = snapshot.source_len.div_ceil(64 * 1024) * 2 + 8;
    let plain_runs = snapshot
        .facts
        .iter()
        .filter(|fact| matches!(fact.kind, InlineKind::Text | InlineKind::SoftBreak))
        .count();
    if plain_runs > maximum_runs {
        return Err(format!(
            "plain source produced {plain_runs} text/break records; maximum is {maximum_runs}"
        ));
    }
    validate_resources(&snapshot.resources, ResourceCaps::PLAIN)
}

pub fn reference_fanout_source(consumers: usize, destination: &str) -> String {
    let mut source = String::with_capacity(consumers * 8 + destination.len() + 16);
    for _ in 0..consumers {
        source.push_str("[label] ");
    }
    source.push_str("\n\n[label]: ");
    source.push_str(destination);
    source.push('\n');
    source
}

/// One shortcut reference per paragraph. Unlike `reference_fanout_source`,
/// this creates thousands of distinct dependent leaves so a compact global
/// dependency refresh cannot hide behind one oversized leaf root.
pub fn reference_multileaf_fanout_source(consumers: usize, definition: Option<&str>) -> String {
    let mut source = String::with_capacity(consumers * 10 + 32);
    for _ in 0..consumers {
        source.push_str("[label]\n\n");
    }
    if let Some(destination) = definition {
        source.push_str("[label]: ");
        source.push_str(destination);
        source.push('\n');
    }
    source
}

pub fn validate_label_task_seam(source: &str, snapshot: &Snapshot) -> Result<(), String> {
    if source.contains("- [x]")
        && snapshot
            .label_dependencies
            .iter()
            .any(|dependency| dependency.normalized_label == "x")
    {
        return Err("task marker entered the reference dependency index".to_owned());
    }
    let definition = snapshot
        .definitions
        .iter()
        .find(|definition| definition.normalized_label == "foo bar")
        .ok_or_else(|| "shared label normalization did not produce `foo bar`".to_owned())?;
    let uses = snapshot
        .reference_uses
        .iter()
        .filter(|reference_use| reference_use.symbol == definition.symbol)
        .collect::<Vec<_>>();
    if uses.len() != 1 {
        return Err(format!(
            "expected one normalized reference consumer, got {}",
            uses.len()
        ));
    }
    let label = source
        .get(uses[0].label_source.clone())
        .ok_or_else(|| "invalid normalized label range".to_owned())?;
    if label != "Foo\t Bar" && label != "FOO  BAR" && label != "foo bar" {
        return Err(format!(
            "task marker was mistaken for reference label: {label:?}"
        ));
    }
    Ok(())
}

pub fn run_stress_edit<E: GateBEngine>(case: &StressEditCase) -> Result<HistoryReport, String> {
    let mut engine = E::open(SyntaxProfile::FlarkGfm, &case.source).map_err(engine_error)?;
    let before = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&case.source, &before)?;
    validate_resources(&before.resources, case.caps)?;
    let expected = case.first_edit.apply(&case.source)?;
    let begin = engine
        .begin_edit(case.first_edit.clone())
        .map_err(engine_error)?;
    validate_phase_evidence(&begin.phase, "begin_edit")?;
    let mut report = HistoryReport {
        revisions: 1,
        ..HistoryReport::default()
    };
    loop {
        let poll = engine.poll(WorkFuel::GATE_B).map_err(engine_error)?;
        validate_poll_evidence(&poll, WorkFuel::GATE_B, begin.token)?;
        // This call is deliberately inside the pending loop: accounting only
        // the committed snapshot would miss the peak old+candidate shape.
        validate_resources(&engine.resource_metrics(), case.caps)?;
        report.polls += 1;
        match poll.receipt.status {
            PollStatus::Pending => report.pending_polls += 1,
            PollStatus::Ready => break,
            PollStatus::Cancelled => return Err("stress edit unexpectedly cancelled".to_owned()),
        }
        if report.polls > 20_000_000 {
            return Err(format!("{} exceeded poll safety bound", case.name));
        }
    }
    if case.require_pending && report.pending_polls == 0 {
        return Err(format!(
            "{} completed as one indivisible inline operation",
            case.name
        ));
    }
    let commit = engine.commit().map_err(engine_error)?;
    validate_phase_evidence(&commit.phase, "commit")?;
    if commit.token != begin.token {
        return Err("stress edit committed the wrong generation".to_owned());
    }
    let after = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&expected, &after)?;
    validate_resources(&after.resources, case.caps)?;
    let clean = E::clean_snapshot(SyntaxProfile::FlarkGfm, &expected).map_err(engine_error)?;
    validate_clean_equivalence(&after, &clean)?;
    validate_replayable_delta(&before, &after, &case.first_edit, &commit.delta)?;
    report.revisions = 2;
    report.maximum_delta_bytes = commit.delta.encoded_bytes;
    Ok(report)
}

pub fn run_supersession_case<E: GateBEngine>(case: &StressEditCase) -> Result<(), String> {
    let mut engine = E::open(SyntaxProfile::FlarkGfm, &case.source).map_err(engine_error)?;
    let before = engine.snapshot().map_err(engine_error)?;
    let first = engine
        .begin_edit(case.first_edit.clone())
        .map_err(engine_error)?;
    validate_phase_evidence(&first.phase, "begin_edit")?;
    let first_poll = engine.poll(WorkFuel::GATE_B).map_err(engine_error)?;
    validate_poll_evidence(&first_poll, WorkFuel::GATE_B, first.token)?;
    if first_poll.receipt.status != PollStatus::Pending {
        return Err("supersession fixture did not leave cancellable work".to_owned());
    }
    validate_resources(&engine.resource_metrics(), case.caps)?;

    let superseded = engine
        .supersede_edit(case.superseding_edit.clone())
        .map_err(engine_error)?;
    validate_phase_evidence(&superseded.started.phase, "supersede_edit")?;
    if superseded.cancelled != first.token {
        return Err("supersede_edit did not cancel the active generation".to_owned());
    }
    if superseded.started.token.revision != first.token.revision
        || superseded.started.token.generation <= first.token.generation
    {
        return Err("supersede_edit did not create a newer generation".to_owned());
    }
    loop {
        let poll = engine.poll(WorkFuel::GATE_B).map_err(engine_error)?;
        validate_poll_evidence(&poll, WorkFuel::GATE_B, superseded.started.token)?;
        validate_resources(&engine.resource_metrics(), case.caps)?;
        match poll.receipt.status {
            PollStatus::Pending => {}
            PollStatus::Ready => break,
            PollStatus::Cancelled => return Err("latest generation was cancelled".to_owned()),
        }
    }
    let commit = engine.commit().map_err(engine_error)?;
    validate_phase_evidence(&commit.phase, "commit")?;
    if commit.token != superseded.started.token || commit.token == first.token {
        return Err("stale generation became committable".to_owned());
    }
    let expected = case.superseding_edit.apply(&case.source)?;
    if engine.materialize_source() != expected {
        return Err("supersession committed stale source".to_owned());
    }
    let after = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&expected, &after)?;
    let clean = E::clean_snapshot(SyntaxProfile::FlarkGfm, &expected).map_err(engine_error)?;
    validate_clean_equivalence(&after, &clean)?;
    validate_replayable_delta(&before, &after, &case.superseding_edit, &commit.delta)
}

pub fn run_plain_coalescing_case<E: GateBEngine>(lines: usize) -> Result<(), String> {
    let source = plain_softbreak_source(lines);
    let engine = E::open(SyntaxProfile::FlarkGfm, &source).map_err(engine_error)?;
    let snapshot = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&source, &snapshot)?;
    validate_plain_run_coalescing(&snapshot)
}

pub fn run_reference_fanout_case<E: GateBEngine>(consumers: usize) -> Result<(), String> {
    let source = reference_fanout_source(consumers, "/old");
    let mut engine = E::open(SyntaxProfile::FlarkGfm, &source).map_err(engine_error)?;
    let before = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&source, &before)?;
    validate_resources(&before.resources, ResourceCaps::REFERENCE_FANOUT)?;
    if before.reference_uses.len() != consumers {
        return Err(format!(
            "reference fanout produced {} consumers, expected {consumers}",
            before.reference_uses.len()
        ));
    }
    let old_start = source
        .rfind("/old")
        .ok_or_else(|| "reference destination missing".to_owned())?;
    let edit = Edit {
        base_revision: before.revision,
        start_utf8: old_start,
        end_utf8: old_start + 4,
        replacement: "/new".to_owned(),
    };
    let expected = edit.apply(&source)?;
    let begin = engine.begin_edit(edit.clone()).map_err(engine_error)?;
    validate_phase_evidence(&begin.phase, "begin_edit")?;
    loop {
        let poll = engine.poll(WorkFuel::GATE_B).map_err(engine_error)?;
        validate_poll_evidence(&poll, WorkFuel::GATE_B, begin.token)?;
        validate_resources(&engine.resource_metrics(), ResourceCaps::REFERENCE_FANOUT)?;
        match poll.receipt.status {
            PollStatus::Pending => {}
            PollStatus::Ready => break,
            PollStatus::Cancelled => return Err("reference edit unexpectedly cancelled".to_owned()),
        }
    }
    let commit = engine.commit().map_err(engine_error)?;
    validate_phase_evidence(&commit.phase, "commit")?;
    let after = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&expected, &after)?;
    validate_replayable_delta(&before, &after, &edit, &commit.delta)?;
    let clean = E::clean_snapshot(SyntaxProfile::FlarkGfm, &expected).map_err(engine_error)?;
    validate_clean_equivalence(&after, &clean)?;

    let before_facts = before
        .facts
        .iter()
        .map(|fact| fact.id)
        .collect::<BTreeSet<_>>();
    let after_facts = after
        .facts
        .iter()
        .map(|fact| fact.id)
        .collect::<BTreeSet<_>>();
    let before_uses = before
        .reference_uses
        .iter()
        .map(|reference_use| (reference_use.fact, reference_use.symbol))
        .collect::<BTreeSet<_>>();
    let after_uses = after
        .reference_uses
        .iter()
        .map(|reference_use| (reference_use.fact, reference_use.symbol))
        .collect::<BTreeSet<_>>();
    if before_facts != after_facts || before_uses != after_uses {
        return Err("definition target edit churned reference consumers".to_owned());
    }
    if !commit.delta.removed_fact_ids.is_empty()
        || !commit.delta.upserted_facts.is_empty()
        || !commit.delta.removed_reference_use_facts.is_empty()
        || !commit.delta.upserted_reference_uses.is_empty()
        || commit.delta.upserted_definitions.len() != 1
        || commit.delta.output_sequence_refresh.is_some()
        || !commit.delta.dependency_index_refreshes.is_empty()
    {
        return Err("definition target edit rewrote consumers instead of one symbol".to_owned());
    }
    if before.output_sequence_root != after.output_sequence_root
        || before.label_dependencies != after.label_dependencies
        || before.dependency_indexes != after.dependency_indexes
    {
        return Err("definition value edit churned output/dependency roots".to_owned());
    }
    if commit.delta.upserted_definitions[0].symbol != before.definitions[0].symbol {
        return Err("definition target edit churned the symbol ID".to_owned());
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReferencePresenceReport {
    pub transitions: usize,
    pub polls: usize,
    pub pending_polls: usize,
    pub reference_resolution_transitions: usize,
    pub maximum_delta_bytes: usize,
}

pub fn run_reference_presence_fanout_case<E: GateBEngine>(
    consumers: usize,
) -> Result<ReferencePresenceReport, String> {
    let defined_source = reference_multileaf_fanout_source(consumers, Some("/winner"));
    let mut engine = E::open(SyntaxProfile::FlarkGfm, &defined_source).map_err(engine_error)?;
    let defined = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&defined_source, &defined)?;
    validate_reference_presence_snapshot(&defined, consumers, true)?;
    validate_resources(&defined.resources, ResourceCaps::REFERENCE_FANOUT)?;

    let definition_start = defined_source
        .rfind("[label]: /winner\n")
        .ok_or_else(|| "fanout definition missing".to_owned())?;
    let removal = Edit {
        base_revision: defined.revision,
        start_utf8: definition_start,
        end_utf8: defined_source.len(),
        replacement: String::new(),
    };
    let undefined_source = removal.apply(&defined_source)?;
    let begin = engine.begin_edit(removal.clone()).map_err(engine_error)?;
    validate_phase_evidence(&begin.phase, "begin_edit")?;
    validate_resources(&engine.resource_metrics(), ResourceCaps::REFERENCE_FANOUT)?;
    let mut report = ReferencePresenceReport::default();
    drive_reference_fanout::<E>(&mut engine, begin.token, consumers, &mut report)?;
    let removal_commit = engine.commit().map_err(engine_error)?;
    validate_phase_evidence(&removal_commit.phase, "commit")?;
    let undefined = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&undefined_source, &undefined)?;
    validate_reference_presence_snapshot(&undefined, consumers, false)?;
    validate_resources(&undefined.resources, ResourceCaps::REFERENCE_FANOUT)?;
    validate_presence_delta(
        &defined,
        &undefined,
        &removal,
        &removal_commit.delta,
        consumers,
    )?;
    let clean =
        E::clean_snapshot(SyntaxProfile::FlarkGfm, &undefined_source).map_err(engine_error)?;
    validate_clean_equivalence(&undefined, &clean)?;
    report.maximum_delta_bytes = report
        .maximum_delta_bytes
        .max(removal_commit.delta.encoded_bytes);
    report.transitions += 1;

    // Re-addition is a separate proof: the undefined snapshot must retain all
    // misses, otherwise the parser has no bounded way to discover dependents.
    let addition = Edit {
        base_revision: undefined.revision,
        start_utf8: undefined_source.len(),
        end_utf8: undefined_source.len(),
        replacement: "[label]: /winner\n".to_owned(),
    };
    if addition.apply(&undefined_source)? != defined_source {
        return Err("presence fanout re-addition does not restore source".to_owned());
    }
    let begin = engine.begin_edit(addition.clone()).map_err(engine_error)?;
    validate_phase_evidence(&begin.phase, "begin_edit")?;
    validate_resources(&engine.resource_metrics(), ResourceCaps::REFERENCE_FANOUT)?;
    drive_reference_fanout::<E>(&mut engine, begin.token, consumers, &mut report)?;
    let addition_commit = engine.commit().map_err(engine_error)?;
    validate_phase_evidence(&addition_commit.phase, "commit")?;
    let redefined = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&defined_source, &redefined)?;
    validate_reference_presence_snapshot(&redefined, consumers, true)?;
    validate_resources(&redefined.resources, ResourceCaps::REFERENCE_FANOUT)?;
    validate_presence_delta(
        &undefined,
        &redefined,
        &addition,
        &addition_commit.delta,
        consumers,
    )?;
    let clean =
        E::clean_snapshot(SyntaxProfile::FlarkGfm, &defined_source).map_err(engine_error)?;
    validate_clean_equivalence(&redefined, &clean)?;
    report.maximum_delta_bytes = report
        .maximum_delta_bytes
        .max(addition_commit.delta.encoded_bytes);
    report.transitions += 1;
    Ok(report)
}

fn drive_reference_fanout<E: GateBEngine>(
    engine: &mut E,
    token: PendingToken,
    consumers: usize,
    report: &mut ReferencePresenceReport,
) -> Result<(), String> {
    let pending_before = report.pending_polls;
    let resolution_before = report.reference_resolution_transitions;
    let polls_before = report.polls;
    loop {
        let poll = engine.poll(WorkFuel::GATE_B).map_err(engine_error)?;
        validate_poll_evidence(&poll, WorkFuel::GATE_B, token)?;
        validate_resources(&engine.resource_metrics(), ResourceCaps::REFERENCE_FANOUT)?;
        report.polls += 1;
        report.reference_resolution_transitions += poll
            .events
            .iter()
            .filter_map(|event| match event {
                AuditEvent::GrammarTransitions {
                    kind: TransitionKind::ReferenceResolution,
                    count,
                } => Some(*count),
                _ => None,
            })
            .sum::<usize>();
        match poll.receipt.status {
            PollStatus::Pending => report.pending_polls += 1,
            PollStatus::Ready => break,
            PollStatus::Cancelled => {
                return Err("reference-presence transition unexpectedly cancelled".to_owned())
            }
        }
        if report.polls - polls_before > 1_000_000 {
            return Err("reference-presence transition exceeded poll safety bound".to_owned());
        }
    }
    if report.pending_polls == pending_before {
        return Err("global reference-presence transition never yielded".to_owned());
    }
    if report.reference_resolution_transitions - resolution_before < consumers {
        return Err(format!(
            "only {} reference-resolution transitions for {consumers} dependent leaves",
            report.reference_resolution_transitions - resolution_before
        ));
    }
    Ok(())
}

fn validate_reference_presence_snapshot(
    snapshot: &Snapshot,
    consumers: usize,
    defined: bool,
) -> Result<(), String> {
    let dependencies = snapshot
        .label_dependencies
        .iter()
        .filter(|dependency| dependency.normalized_label == "label")
        .collect::<Vec<_>>();
    let dependent_leaves = dependencies
        .iter()
        .map(|dependency| dependency.leaf)
        .collect::<BTreeSet<_>>();
    let index = snapshot
        .dependency_indexes
        .iter()
        .find(|index| index.normalized_label == "label")
        .ok_or_else(|| "reference fanout lacks a global dependency index".to_owned())?;
    if dependencies.len() != consumers
        || dependent_leaves.len() != consumers
        || dependencies
            .iter()
            .any(|dependency| dependency.occurrences != 1)
        || index.dependent_leaf_count != consumers
        || index.occurrences != consumers
    {
        return Err("reference fanout did not retain one dependency per leaf".to_owned());
    }

    let definitions = snapshot
        .definitions
        .iter()
        .filter(|definition| definition.normalized_label == "label")
        .collect::<Vec<_>>();
    if defined {
        if definitions.len() != 1
            || snapshot.reference_uses.len() != consumers
            || index.resolved_symbol != Some(definitions[0].symbol)
            || dependencies
                .iter()
                .any(|dependency| dependency.resolved_symbol != Some(definitions[0].symbol))
            || snapshot
                .reference_uses
                .iter()
                .any(|reference_use| reference_use.symbol != definitions[0].symbol)
        {
            return Err("defined fanout has stale/missing consumers or symbol".to_owned());
        }
    } else if !definitions.is_empty()
        || !snapshot.reference_uses.is_empty()
        || index.resolved_symbol.is_some()
        || dependencies
            .iter()
            .any(|dependency| dependency.resolved_symbol.is_some())
        || snapshot.facts.iter().any(|fact| {
            matches!(
                fact.kind,
                InlineKind::Link { .. } | InlineKind::Image { .. }
            )
        })
    {
        return Err("undefined fanout retained stale link recognition/symbols".to_owned());
    }
    Ok(())
}

pub fn validate_presence_delta(
    before: &Snapshot,
    after: &Snapshot,
    edit: &Edit,
    delta: &RevisionDelta,
    consumers: usize,
) -> Result<(), String> {
    validate_replayable_delta(before, after, edit, delta)?;
    if delta.encoded_bytes > 64 * 1024
        || !delta.removed_leaf_ids.is_empty()
        || !delta.upserted_leaves.is_empty()
        || !delta.leaf_order_splices.is_empty()
        || !delta.removed_fact_ids.is_empty()
        || !delta.upserted_facts.is_empty()
        || !delta.fact_order_splices.is_empty()
        || !delta.removed_reference_use_facts.is_empty()
        || !delta.upserted_reference_uses.is_empty()
        || !delta.removed_label_dependencies.is_empty()
        || !delta.upserted_label_dependencies.is_empty()
    {
        return Err("presence fanout enumerated thousands of dependent outputs".to_owned());
    }
    let output = delta
        .output_sequence_refresh
        .as_ref()
        .ok_or_else(|| "presence fanout lacks output-sequence refresh".to_owned())?;
    if output.affected_leaf_count != consumers {
        return Err(format!(
            "output refresh covers {} leaves, expected {consumers}",
            output.affected_leaf_count
        ));
    }
    if delta.dependency_index_refreshes.len() != 1
        || delta.dependency_index_refreshes[0].normalized_label != "label"
    {
        return Err("presence fanout lacks one compact dependency-generation refresh".to_owned());
    }
    Ok(())
}

pub fn run_inline_locality_case<E: GateBEngine>() -> Result<(), String> {
    let case = inline_locality_case();
    let mut engine = E::open(SyntaxProfile::FlarkGfm, &case.source).map_err(engine_error)?;
    let before = engine.snapshot().map_err(engine_error)?;
    let expected = case.edit.apply(&case.source)?;
    let begin = engine.begin_edit(case.edit.clone()).map_err(engine_error)?;
    validate_phase_evidence(&begin.phase, "begin_edit")?;
    loop {
        let poll = engine.poll(WorkFuel::GATE_B).map_err(engine_error)?;
        validate_poll_evidence(&poll, WorkFuel::GATE_B, begin.token)?;
        match poll.receipt.status {
            PollStatus::Pending => {}
            PollStatus::Ready => break,
            PollStatus::Cancelled => return Err("local edit unexpectedly cancelled".to_owned()),
        }
    }
    let commit = engine.commit().map_err(engine_error)?;
    validate_phase_evidence(&commit.phase, "commit")?;
    let after = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&expected, &after)?;
    validate_replayable_delta(&before, &after, &case.edit, &commit.delta)?;
    validate_protected_identity(&before, &after, &case.edit, &commit.delta, &case.protected)?;
    let clean = E::clean_snapshot(SyntaxProfile::FlarkGfm, &expected).map_err(engine_error)?;
    validate_clean_equivalence(&after, &clean)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GateBReport {
    pub fixtures: usize,
    pub history_revisions: usize,
    pub history_polls: usize,
    pub stress_polls: usize,
}

/// Full acceptance entry point. Expensive lanes should be run in isolated
/// subprocesses with external wall/RSS/allocator instrumentation in CI.
pub fn run_gate_b<E: GateBEngine>(repo_root: &Path) -> Result<GateBReport, String> {
    let fixtures = run_fixture_corpus::<E>(repo_root)?;
    let mut report = GateBReport {
        fixtures: fixtures.passed,
        ..GateBReport::default()
    };
    for history in gate_b_histories() {
        let history_report = run_history::<E>(&history, ResourceCaps::TOKEN_DENSE)?;
        report.history_revisions += history_report.revisions;
        report.history_polls += history_report.polls;
    }
    for case in giant_inline_cases() {
        let stress = run_stress_edit::<E>(&case)?;
        run_supersession_case::<E>(&case)?;
        report.stress_polls += stress.polls;
    }
    run_plain_coalescing_case::<E>(1_000_000)?;
    run_reference_fanout_case::<E>(5_000)?;
    run_reference_presence_fanout_case::<E>(5_000)?;
    run_inline_locality_case::<E>()?;
    Ok(report)
}
