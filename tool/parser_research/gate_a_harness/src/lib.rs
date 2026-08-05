//! Executable acceptance contract for RFC 023 Gate A.
//!
//! This crate deliberately does not contain a Markdown parser. A prospective
//! Flark core implements [\`GateAEngine\`]; the harness then checks normative
//! fixtures plus candidate-clean equality at every revision, persistent source
//! coverage, direct deltas, bounded polling, memory accounting, and stable-order
//! histories. Comrak and Pulldown are differential peers, not normative
//! authorities.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::ops::Range;
use std::path::Path;

use comrak::{format_html, parse_document, Arena, Options};
use pulldown_cmark::{html as pulldown_html, Options as PulldownOptions, Parser as PulldownParser};
use serde::Deserialize;

pub const COMMONMARK_VERSION: &str = "0.31.2";
pub const GFM_VERSION: &str = "0.29";
pub const COMRAK_VERSION: &str = "0.54.0";
/// Oversized semantic leaves still need bounded representation pages so a
/// local edit cannot hide behind one mutable whole-document coverage record.
pub const MAX_COVERAGE_CHUNK_BYTES: usize = 64 * 1024;

/// The block sections that must be exact before Gate A is allowed to pass.
pub const COMMONMARK_GATE_A_SECTIONS: &[&str] = &[
    "Tabs",
    "Setext headings",
    "HTML blocks",
    "Block quotes",
    "List items",
    "Lists",
];

pub const GFM_GATE_A_SECTIONS: &[&str] = &["Tables (extension)"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxProfile {
    CommonMark0312,
    FlarkGfm,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SpecFixture {
    pub markdown: String,
    pub html: String,
    pub example: usize,
    pub section: String,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateFixture {
    pub authority: FixtureAuthority,
    pub fixture: SpecFixture,
}

pub fn load_gate_a_fixtures(repo_root: &Path) -> Result<Vec<GateFixture>, String> {
    let commonmark =
        load_fixtures(&repo_root.join("test/fixtures/commonmark/upstream/common_mark_tests.json"))?;
    let gfm = load_fixtures(&repo_root.join("test/fixtures/commonmark/upstream/gfm_tests.json"))?;
    let mut selected = commonmark
        .into_iter()
        .filter(|fixture| COMMONMARK_GATE_A_SECTIONS.contains(&fixture.section.as_str()))
        .map(|fixture| GateFixture {
            authority: FixtureAuthority::CommonMark0312,
            fixture,
        })
        .collect::<Vec<_>>();
    selected.extend(
        gfm.into_iter()
            .filter(|fixture| GFM_GATE_A_SECTIONS.contains(&fixture.section.as_str()))
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

pub fn run_fixture_corpus<E: GateAEngine>(
    repo_root: &Path,
    caps: ResourceCaps,
) -> Result<FixtureReport, String> {
    let fixtures = load_gate_a_fixtures(repo_root)?;
    let mut report = FixtureReport::default();
    for gate in fixtures {
        let profile = gate.authority.profile();
        let engine = E::open(profile, &gate.fixture.markdown).map_err(engine_error)?;
        let snapshot = engine.snapshot().map_err(engine_error)?;
        validate_snapshot(&gate.fixture.markdown, &snapshot)?;
        if snapshot.normalized_html != gate.fixture.html {
            return Err(format!(
                "{:?} example {} ({}) differs from normative HTML",
                gate.authority, gate.fixture.example, gate.fixture.section
            ));
        }
        let clean = E::clean_snapshot(profile, &gate.fixture.markdown).map_err(engine_error)?;
        validate_clean_equivalence(&snapshot, &clean)?;
        validate_resources(&snapshot, None, caps)?;
        report.passed += 1;
        *report.by_section.entry(gate.fixture.section).or_default() += 1;
    }
    Ok(report)
}

/// Independent clean renderer for differential evidence. Normative fixture HTML
/// and the pinned Flark profile, not this implementation, decide correctness.
pub fn oracle_html(source: &str) -> Result<String, String> {
    oracle_html_for(SyntaxProfile::FlarkGfm, source)
}

pub fn oracle_html_for(profile: SyntaxProfile, source: &str) -> Result<String, String> {
    let arena = Arena::new();
    let options = gate_a_options(profile);
    let root = parse_document(&arena, source, &options);
    let mut html = String::new();
    format_html(root, &options, &mut html).map_err(|error| error.to_string())?;
    Ok(html)
}

/// Independent clean renderer used to ensure revision histories are not
/// accidentally defined by the selected donor implementation.
pub fn pulldown_oracle_html_for(profile: SyntaxProfile, source: &str) -> String {
    let mut options = PulldownOptions::empty();
    if profile == SyntaxProfile::FlarkGfm {
        options.insert(PulldownOptions::ENABLE_TABLES);
        options.insert(PulldownOptions::ENABLE_STRIKETHROUGH);
        options.insert(PulldownOptions::ENABLE_TASKLISTS);
        options.insert(PulldownOptions::ENABLE_GFM);
    }
    let parser = PulldownParser::new_ext(source, options);
    let mut html = String::new();
    pulldown_html::push_html(&mut html, parser);
    html
}

fn gate_a_options(profile: SyntaxProfile) -> Options<'static> {
    let mut options = Options::default();
    options.render.r#unsafe = true;
    if profile == SyntaxProfile::FlarkGfm {
        options.extension.table = true;
        options.extension.strikethrough = true;
        options.extension.autolink = true;
        options.extension.tagfilter = true;
        options.extension.tasklist = true;
    }
    options
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
            return Err("edit range is not on UTF-8 scalar boundaries".to_owned());
        }
        if self.replacement.contains('\r') {
            return Err("Gate A input must be LF-normalized".to_owned());
        }
        let mut edited = source.to_owned();
        edited.replace_range(self.start_utf8..self.end_utf8, &self.replacement);
        Ok(edited)
    }
}

/// Produces the smallest scalar-safe single splice that turns \`before\` into
/// \`after\`. This is also used to turn target-source histories into realistic
/// editor transactions.
pub fn minimal_edit(base_revision: u64, before: &str, after: &str) -> Edit {
    let prefix = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    let mut prefix = prefix;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionExpectation {
    pub source: String,
    pub facts: Vec<FactProbe>,
}

impl RevisionExpectation {
    pub fn source(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            facts: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditHistory {
    pub name: &'static str,
    pub profile: SyntaxProfile,
    pub revisions: Vec<RevisionExpectation>,
    /// If true, at least one poll must return \`Pending\`. This prevents a giant
    /// physical line from being admitted as an indivisible unit of work.
    pub require_pending: bool,
}

/// Focused ambiguity histories. Typing cases include every scalar revision, so
/// the implementation cannot be correct only at final syntax.
pub fn gate_a_histories() -> Vec<EditHistory> {
    let mut histories = vec![
        typing_and_erasing_history(
            "quote-list-tab-every-revision",
            "> - item\n>\tcontinued\nlazy\n\nend\n",
        ),
        typing_and_erasing_history("setext-every-revision", "heading\n=======\n\nafter\n"),
        typing_and_erasing_history(
            "table-every-revision",
            "| a | \x60b\\|c\x60 |\n| :- | -: |\n| d | e |\n\nafter\n",
        ),
        typing_and_erasing_history(
            "html-classes-every-revision",
            concat!(
                "<script>x</script>\n\n",
                "<!-- comment -->\n\n",
                "<?processing?>\n\n",
                "<!DECLARATION>\n\n",
                "<![CDATA[data]]>\n\n",
                "<div>\nraw *body*\n\n",
                "<i>raw</i>\n\n"
            ),
        ),
        explicit_history(
            "lazy-quote-interruption-toggle",
            &[
                "> paragraph\ncontinuation\n\nafter\n",
                "> paragraph\n#continuation\n\nafter\n",
                "> paragraph\n# continuation\n\nafter\n",
                "> paragraph\ncontinuation\n\nafter\n",
            ],
        ),
        explicit_history(
            "list-tightness-toggle",
            &[
                "- a\n- b\n\nafter\n",
                "- a\n\n- b\n\nafter\n",
                "- a\n- b\n\nafter\n",
            ],
        ),
        explicit_history(
            "setext-validity-toggle",
            &[
                "title\n=====\n\nafter\n",
                "title\n==x==\n\nafter\n",
                "title\n=====\n\nafter\n",
                "changed\n=====\n\nafter\n",
            ],
        ),
        explicit_history(
            "table-validity-toggle",
            &[
                "| a | b |\n| -- | -- |\n| c | d |\n\nafter\n",
                "| a | b |\n| x- | -- |\n| c | d |\n\nafter\n",
                "| a | b |\n| -- | -- |\n| c | d |\n\nafter\n",
                "| a | b |\n| -- | -- |\n> quote\n\nafter\n",
            ],
        ),
        explicit_history(
            "tab-indentation-toggle",
            &[
                "- foo\n\n\tbar\n\nafter\n",
                "- foo\n\n   bar\n\nafter\n",
                "- foo\n\n    bar\n\nafter\n",
                "- foo\n\n\tbar\n\nafter\n",
            ],
        ),
        explicit_history(
            "html-close-toggle",
            &[
                "<!--\n*inert*\n--x\nafter\n",
                "<!--\n*inert*\n-->\nafter\n",
                "<!--\n*inert*\n--x\nafter\n",
            ],
        ),
    ];

    attach_probe(
        &mut histories[4].revisions[0],
        FactKind::BlockQuote,
        "> paragraph\ncontinuation",
        &[">"],
    );
    attach_probe(
        &mut histories[5].revisions[0],
        FactKind::List {
            ordered: false,
            start: 1,
            tight: true,
        },
        "- a\n- b",
        &["-", "-"],
    );
    attach_probe(
        &mut histories[5].revisions[1],
        FactKind::List {
            ordered: false,
            start: 1,
            tight: false,
        },
        "- a\n\n- b",
        &["-", "-"],
    );
    for revision in [0, 2] {
        attach_probe(
            &mut histories[6].revisions[revision],
            FactKind::Heading {
                level: 1,
                setext: true,
            },
            "title\n=====",
            &["====="],
        );
    }
    for revision in [0, 2] {
        attach_probe(
            &mut histories[7].revisions[revision],
            FactKind::TableCell { column: 0 },
            "a",
            &[],
        );
    }
    attach_probe(
        &mut histories[8].revisions[0],
        FactKind::ListItem,
        "- foo\n\n\tbar",
        &["-"],
    );
    attach_probe(
        &mut histories[9].revisions[1],
        FactKind::HtmlBlock(HtmlBlockClass::Comment),
        "<!--\n*inert*\n-->",
        &["<!--", "-->"],
    );
    histories
}

fn attach_probe(
    revision: &mut RevisionExpectation,
    kind: FactKind,
    source_text: &str,
    marker_texts: &[&str],
) {
    revision.facts.push(FactProbe {
        kind,
        source_text: source_text.to_owned(),
        marker_texts: marker_texts
            .iter()
            .map(|marker| (*marker).to_owned())
            .collect(),
    });
}

fn explicit_history(name: &'static str, sources: &[&str]) -> EditHistory {
    EditHistory {
        name,
        profile: SyntaxProfile::FlarkGfm,
        revisions: sources
            .iter()
            .map(|source| RevisionExpectation::source(*source))
            .collect(),
        require_pending: false,
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
        require_pending: false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StableId(pub u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HtmlBlockClass {
    ScriptPreStyleTextarea,
    Comment,
    ProcessingInstruction,
    Declaration,
    Cdata,
    BlockTag,
    CompleteTag,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FactKind {
    BlockQuote,
    List {
        ordered: bool,
        start: u64,
        tight: bool,
    },
    ListItem,
    Paragraph,
    Heading {
        level: u8,
        setext: bool,
    },
    Table {
        columns: u32,
    },
    TableRow,
    TableCell {
        column: u32,
    },
    HtmlBlock(HtmlBlockClass),
    Blank,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxFact {
    pub id: StableId,
    pub parent: Option<StableId>,
    pub kind: FactKind,
    /// Exact syntax extent, excluding an otherwise-semantic-free terminal line
    /// ending. Coverage chunks still partition every byte including that line
    /// ending.
    pub source: Range<usize>,
    pub markers: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverageChunk {
    pub id: StableId,
    pub source: Range<usize>,
    /// A coverage kind is representation-level, not a second grammar verdict.
    pub kind: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResourceMetrics {
    pub source_backing_bytes: usize,
    pub checkpoint_bytes: usize,
    pub chunk_index_bytes: usize,
    pub fact_index_bytes: usize,
    pub ordering_bytes: usize,
    pub retained_history_bytes: usize,
    pub retained_source_prefix_bytes: usize,
    pub peak_scratch_bytes: usize,
}

impl ResourceMetrics {
    pub fn persistent_auxiliary_bytes(&self) -> usize {
        self.checkpoint_bytes
            .saturating_add(self.chunk_index_bytes)
            .saturating_add(self.fact_index_bytes)
            .saturating_add(self.ordering_bytes)
            .saturating_add(self.retained_history_bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub profile: SyntaxProfile,
    pub revision: u64,
    pub source_len: usize,
    pub source_hash64: u64,
    pub normalized_html: String,
    pub coverage: Vec<CoverageChunk>,
    pub facts: Vec<SyntaxFact>,
    pub resources: ResourceMetrics,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactProbe {
    pub kind: FactKind,
    pub source_text: String,
    pub marker_texts: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkFuel {
    pub max_source_bytes: usize,
    pub max_transitions: usize,
}

impl WorkFuel {
    pub const fn new(max_source_bytes: usize, max_transitions: usize) -> Self {
        Self {
            max_source_bytes,
            max_transitions,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollStatus {
    Pending,
    Ready,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollReceipt {
    pub status: PollStatus,
    pub source_bytes_examined: usize,
    pub transitions: usize,
    pub cancellation_checks: usize,
    /// Longest uninterrupted run between explicit cancellation checks.
    pub max_uninterrupted_transitions: usize,
    pub peak_scratch_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionDelta {
    pub base_revision: u64,
    pub revision: u64,
    pub removed_coverage_ids: Vec<StableId>,
    pub inserted_coverage: Vec<CoverageChunk>,
    pub removed_fact_ids: Vec<StableId>,
    pub upserted_facts: Vec<SyntaxFact>,
    pub encoded_bytes: usize,
    /// These receipts are audited implementation claims, not optimization
    /// hints. Either being non-zero fails the architectural gate.
    pub batch_tree_materializations: usize,
    pub grammar_side_scans: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhaseReceipt {
    pub source_bytes_examined: usize,
    pub grammar_transitions: usize,
    pub structural_steps: usize,
    pub newly_allocated_bytes: usize,
    pub batch_tree_materializations: usize,
    pub grammar_side_scans: usize,
}

/// Candidate adapter. Pending work remains inside the engine so each call to
/// \`poll\` can be independently fuel-checked and cancelled.
///
/// \`open\`, \`snapshot\`, and \`clean_snapshot\` are test-only batch
/// operations. They are never evidence of production liveness. Production
/// evidence starts at \`begin_edit\`: begin may only splice/source-own the
/// request, poll owns all grammar work, and commit may only adopt already-built
/// roots and the direct delta. Their phase receipts are mandatory and are
/// externally wall-time/allocation audited in the final runner.
pub trait GateAEngine: Sized {
    type Error: Display;

    fn open(profile: SyntaxProfile, source: &str) -> Result<Self, Self::Error>;
    fn revision(&self) -> u64;
    fn materialize_source(&self) -> String;
    fn snapshot(&self) -> Result<Snapshot, Self::Error>;
    fn clean_snapshot(profile: SyntaxProfile, source: &str) -> Result<Snapshot, Self::Error>;
    fn resource_metrics(&self) -> ResourceMetrics;
    fn begin_edit(&mut self, edit: Edit) -> Result<PhaseReceipt, Self::Error>;
    fn poll(&mut self, fuel: WorkFuel) -> Result<PollReceipt, Self::Error>;
    fn cancel_pending(&mut self);
    fn discard_pending(&mut self);
    fn commit(&mut self) -> Result<(RevisionDelta, PhaseReceipt), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceCaps {
    pub persistent_auxiliary_bytes: usize,
    pub peak_scratch_bytes: usize,
    pub retained_source_prefix_bytes: usize,
    pub delta_bytes: usize,
}

impl ResourceCaps {
    /// Gate default for pathological 10 MiB / million-line fixtures. This is
    /// intentionally a product ceiling, not a ratio to the tiny 2 MiB
    /// dense-line source: the old prototypes measured 209-473 MiB and must fail
    /// clearly.
    pub const GATE_A: Self = Self {
        persistent_auxiliary_bytes: 64 * 1024 * 1024,
        peak_scratch_bytes: 16 * 1024 * 1024,
        retained_source_prefix_bytes: 64 * 1024,
        delta_bytes: 64 * 1024,
    };

    /// A million soft-break lines are one paragraph, not a justification for a
    /// million independently allocated parser/checkpoint/output records.
    pub const DENSE_LINES: Self = Self {
        persistent_auxiliary_bytes: 16 * 1024 * 1024,
        peak_scratch_bytes: 8 * 1024 * 1024,
        retained_source_prefix_bytes: 64 * 1024,
        delta_bytes: 64 * 1024,
    };

    fn strictest(self, other: Self) -> Self {
        Self {
            persistent_auxiliary_bytes: self
                .persistent_auxiliary_bytes
                .min(other.persistent_auxiliary_bytes),
            peak_scratch_bytes: self.peak_scratch_bytes.min(other.peak_scratch_bytes),
            retained_source_prefix_bytes: self
                .retained_source_prefix_bytes
                .min(other.retained_source_prefix_bytes),
            delta_bytes: self.delta_bytes.min(other.delta_bytes),
        }
    }
}

pub fn validate_poll(receipt: PollReceipt, fuel: WorkFuel) -> Result<(), String> {
    if fuel.max_source_bytes == 0 || fuel.max_transitions == 0 {
        return Err("fuel must be non-zero".to_owned());
    }
    if receipt.source_bytes_examined > fuel.max_source_bytes {
        return Err(format!(
            "poll examined {} bytes with {} byte fuel",
            receipt.source_bytes_examined, fuel.max_source_bytes
        ));
    }
    if receipt.transitions > fuel.max_transitions {
        return Err(format!(
            "poll ran {} transitions with {} transition fuel",
            receipt.transitions, fuel.max_transitions
        ));
    }
    if receipt.max_uninterrupted_transitions > fuel.max_transitions {
        return Err(format!(
            "{} uninterrupted transitions exceeds poll fuel {}",
            receipt.max_uninterrupted_transitions, fuel.max_transitions
        ));
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
    Ok(())
}

pub fn validate_non_poll_phase(receipt: PhaseReceipt, phase: &str) -> Result<(), String> {
    if receipt.source_bytes_examined > 64 {
        return Err(format!(
            "{phase} examined {} source bytes; grammar work belongs in poll",
            receipt.source_bytes_examined
        ));
    }
    if receipt.grammar_transitions != 0 {
        return Err(format!(
            "{phase} ran {} grammar transitions",
            receipt.grammar_transitions
        ));
    }
    if receipt.structural_steps > 256 {
        return Err(format!(
            "{phase} ran {} structural steps",
            receipt.structural_steps
        ));
    }
    if receipt.newly_allocated_bytes > 64 * 1024 {
        return Err(format!(
            "{phase} allocated {} bytes outside poll",
            receipt.newly_allocated_bytes
        ));
    }
    if receipt.batch_tree_materializations != 0 || receipt.grammar_side_scans != 0 {
        return Err(format!(
            "{phase} used batch trees or grammar side scans: {receipt:?}"
        ));
    }
    Ok(())
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
    let mut coverage_ids = BTreeSet::new();
    let mut cursor = 0;
    for chunk in &snapshot.coverage {
        if !coverage_ids.insert(chunk.id) {
            return Err(format!("duplicate coverage ID {:?}", chunk.id));
        }
        validate_range(source, &chunk.source, "coverage")?;
        if chunk.source.len() > MAX_COVERAGE_CHUNK_BYTES {
            return Err(format!(
                "coverage chunk {:?} spans {} bytes; maximum is {}",
                chunk.id,
                chunk.source.len(),
                MAX_COVERAGE_CHUNK_BYTES
            ));
        }
        if chunk.source.start != cursor {
            return Err(format!(
                "coverage gap/overlap at {cursor}; next={:?}",
                chunk.source
            ));
        }
        cursor = chunk.source.end;
    }
    if cursor != source.len() {
        return Err(format!(
            "coverage ended at {cursor}, source ends at {}",
            source.len()
        ));
    }

    let mut fact_ids = BTreeSet::new();
    for fact in &snapshot.facts {
        if !fact_ids.insert(fact.id) {
            return Err(format!("duplicate fact ID {:?}", fact.id));
        }
        validate_range(source, &fact.source, "fact")?;
        for marker in &fact.markers {
            validate_range(source, marker, "marker")?;
            if marker.start < fact.source.start || marker.end > fact.source.end {
                return Err(format!(
                    "marker {marker:?} escapes owner fact {:?}",
                    fact.source
                ));
            }
        }
        if let Some(parent) = fact.parent {
            if !snapshot
                .facts
                .iter()
                .any(|candidate| candidate.id == parent)
            {
                return Err(format!("fact {:?} has missing parent {parent:?}", fact.id));
            }
        }
    }
    let facts_by_id = snapshot
        .facts
        .iter()
        .map(|fact| (fact.id, fact))
        .collect::<BTreeMap<_, _>>();
    for fact in &snapshot.facts {
        let mut ancestors = BTreeSet::new();
        let mut cursor = fact.parent;
        while let Some(parent_id) = cursor {
            if !ancestors.insert(parent_id) || parent_id == fact.id {
                return Err(format!("fact {:?} has cyclic ancestry", fact.id));
            }
            let parent = facts_by_id
                .get(&parent_id)
                .ok_or_else(|| format!("fact {:?} has missing parent {parent_id:?}", fact.id))?;
            if parent.source.start > fact.source.start || fact.source.end > parent.source.end {
                return Err(format!(
                    "fact {:?} range {:?} escapes ancestor {parent_id:?} range {:?}",
                    fact.id, fact.source, parent.source
                ));
            }
            cursor = parent.parent;
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

pub fn validate_resources(
    snapshot: &Snapshot,
    delta: Option<&RevisionDelta>,
    caps: ResourceCaps,
) -> Result<(), String> {
    validate_resource_metrics(&snapshot.resources, caps)?;
    if let Some(delta) = delta {
        validate_delta_receipt(delta, caps)?;
    }
    Ok(())
}

pub fn validate_resource_metrics(
    resources: &ResourceMetrics,
    caps: ResourceCaps,
) -> Result<(), String> {
    if resources.persistent_auxiliary_bytes() > caps.persistent_auxiliary_bytes {
        return Err(format!(
            "persistent auxiliary state {} exceeds {} bytes",
            resources.persistent_auxiliary_bytes(),
            caps.persistent_auxiliary_bytes
        ));
    }
    if resources.peak_scratch_bytes > caps.peak_scratch_bytes {
        return Err(format!(
            "peak scratch {} exceeds {} bytes",
            resources.peak_scratch_bytes, caps.peak_scratch_bytes
        ));
    }
    if resources.retained_source_prefix_bytes > caps.retained_source_prefix_bytes {
        return Err(format!(
            "checkpoint retained {} prefix bytes; cap is {}",
            resources.retained_source_prefix_bytes, caps.retained_source_prefix_bytes
        ));
    }
    Ok(())
}

pub fn validate_delta_receipt(delta: &RevisionDelta, caps: ResourceCaps) -> Result<(), String> {
    if delta.encoded_bytes > caps.delta_bytes {
        return Err(format!(
            "delta encoded {} bytes; cap is {}",
            delta.encoded_bytes, caps.delta_bytes
        ));
    }
    if delta.batch_tree_materializations != 0 {
        return Err(format!(
            "delta path materialized {} batch trees",
            delta.batch_tree_materializations
        ));
    }
    if delta.grammar_side_scans != 0 {
        return Err(format!(
            "delta path ran {} grammar side scans",
            delta.grammar_side_scans
        ));
    }
    Ok(())
}

pub fn validate_fact_probes(
    source: &str,
    snapshot: &Snapshot,
    probes: &[FactProbe],
) -> Result<(), String> {
    for probe in probes {
        let matching = snapshot.facts.iter().find(|fact| {
            fact.kind == probe.kind
                && source.get(fact.source.clone()) == Some(probe.source_text.as_str())
                && fact
                    .markers
                    .iter()
                    .map(|range| &source[range.clone()])
                    .eq(probe.marker_texts.iter().map(String::as_str))
        });
        if matching.is_none() {
            return Err(format!("missing source-fact probe {probe:?}"));
        }
    }
    Ok(())
}

/// Semantic equality intentionally ignores stable IDs and representation byte
/// counts. IDs are checked separately across revisions.
pub fn validate_clean_equivalence(incremental: &Snapshot, clean: &Snapshot) -> Result<(), String> {
    if incremental.profile != clean.profile {
        return Err("incremental/clean syntax profile mismatch".to_owned());
    }
    if incremental.normalized_html != clean.normalized_html {
        return Err("incremental/clean HTML mismatch".to_owned());
    }
    let coverage = |snapshot: &Snapshot| {
        snapshot
            .coverage
            .iter()
            .map(|chunk| (chunk.source.clone(), chunk.kind))
            .collect::<Vec<_>>()
    };
    if coverage(incremental) != coverage(clean) {
        return Err("incremental/clean coverage mismatch".to_owned());
    }
    if semantic_fact_rows(incremental)? != semantic_fact_rows(clean)? {
        return Err("incremental/clean fact mismatch".to_owned());
    }
    Ok(())
}

type SemanticFactRow = (FactKind, Range<usize>, Vec<Range<usize>>, Option<usize>);

fn semantic_fact_rows(snapshot: &Snapshot) -> Result<Vec<SemanticFactRow>, String> {
    let indices = snapshot
        .facts
        .iter()
        .enumerate()
        .map(|(index, fact)| (fact.id, index))
        .collect::<BTreeMap<_, _>>();
    snapshot
        .facts
        .iter()
        .map(|fact| {
            let parent_index =
                fact.parent
                    .map(|parent| {
                        indices.get(&parent).copied().ok_or_else(|| {
                            format!("fact {:?} has missing parent {parent:?}", fact.id)
                        })
                    })
                    .transpose()?;
            Ok((
                fact.kind.clone(),
                fact.source.clone(),
                fact.markers.clone(),
                parent_index,
            ))
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectedRange {
    pub old: Range<usize>,
}

/// Checks stable identity outside a semantic blast radius and ensures a delta
/// only names IDs that actually changed. Ranges after an edit are mapped by the
/// source splice; suffix IDs must not churn just because their byte offsets move.
pub fn validate_local_delta(
    before: &Snapshot,
    after: &Snapshot,
    edit: &Edit,
    delta: &RevisionDelta,
    protected: &[ProtectedRange],
) -> Result<(), String> {
    if delta.base_revision != before.revision || delta.revision != after.revision {
        return Err("delta revision mismatch".to_owned());
    }
    let removed = delta
        .removed_coverage_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let inserted = delta
        .inserted_coverage
        .iter()
        .map(|chunk| chunk.id)
        .collect::<BTreeSet<_>>();
    if removed.len() != delta.removed_coverage_ids.len()
        || inserted.len() != delta.inserted_coverage.len()
    {
        return Err("delta repeats a coverage ID".to_owned());
    }
    let before_by_id = before
        .coverage
        .iter()
        .map(|chunk| (chunk.id, chunk))
        .collect::<BTreeMap<_, _>>();
    let after_by_id = after
        .coverage
        .iter()
        .map(|chunk| (chunk.id, chunk))
        .collect::<BTreeMap<_, _>>();
    let actual_removed = before_by_id
        .keys()
        .filter(|id| !after_by_id.contains_key(id))
        .copied()
        .collect::<BTreeSet<_>>();
    let actual_inserted = after_by_id
        .keys()
        .filter(|id| !before_by_id.contains_key(id))
        .copied()
        .collect::<BTreeSet<_>>();
    if removed != actual_removed {
        return Err(format!(
            "coverage removals differ: delta={removed:?} actual={actual_removed:?}"
        ));
    }
    if inserted != actual_inserted {
        return Err(format!(
            "coverage insertions differ: delta={inserted:?} actual={actual_inserted:?}"
        ));
    }

    // Coverage has no update operation. A retained ID must therefore map
    // unchanged through the source splice; anything intersecting the edit must
    // be explicitly removed and replaced. This makes the delta independently
    // replayable and rejects a single mutable whole-document chunk.
    for (id, before_chunk) in &before_by_id {
        let Some(after_chunk) = after_by_id.get(id) else {
            continue;
        };
        let expected = map_unchanged_range(&before_chunk.source, edit).map_err(|_| {
            format!("coverage ID {id:?} intersects the edit but survived without replacement")
        })?;
        if after_chunk.source != expected || after_chunk.kind != before_chunk.kind {
            return Err(format!(
                "coverage ID {id:?} changed without a replayable remove/insert"
            ));
        }
    }
    for id in &removed {
        if !before_by_id.contains_key(id) || after_by_id.contains_key(id) {
            return Err(format!("removed ID {id:?} is not actually removed"));
        }
    }
    for id in &inserted {
        if before_by_id.contains_key(id) || !after_by_id.contains_key(id) {
            return Err(format!("inserted ID {id:?} is not actually inserted"));
        }
    }

    for range in protected {
        for chunk in before.coverage.iter().filter(|chunk| {
            range.old.start <= chunk.source.start
                && chunk.source.end <= range.old.end
                && (chunk.source.end <= edit.start_utf8 || edit.end_utf8 <= chunk.source.start)
        }) {
            let expected = map_unchanged_range(&chunk.source, edit)?;
            let Some(actual) = after_by_id.get(&chunk.id) else {
                return Err(format!("protected coverage ID {:?} churned", chunk.id));
            };
            if actual.source != expected || actual.kind != chunk.kind {
                return Err(format!(
                    "protected ID {:?} changed from {:?} to {:?}; expected {:?}",
                    chunk.id, chunk.source, actual.source, expected
                ));
            }
            if removed.contains(&chunk.id) || inserted.contains(&chunk.id) {
                return Err(format!("protected ID {:?} appeared in delta", chunk.id));
            }
        }
    }

    let mut replayed_coverage = before
        .coverage
        .iter()
        .filter(|chunk| !removed.contains(&chunk.id))
        .map(|chunk| {
            Ok(CoverageChunk {
                id: chunk.id,
                source: map_unchanged_range(&chunk.source, edit)?,
                kind: chunk.kind,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    replayed_coverage.extend(delta.inserted_coverage.iter().cloned());
    replayed_coverage.sort_by_key(|chunk| (chunk.source.start, chunk.source.end, chunk.id));
    if replayed_coverage != after.coverage {
        return Err("coverage delta does not replay to the committed snapshot".to_owned());
    }

    let before_facts = before
        .facts
        .iter()
        .map(|fact| (fact.id, fact))
        .collect::<BTreeMap<_, _>>();
    let after_facts = after
        .facts
        .iter()
        .map(|fact| (fact.id, fact))
        .collect::<BTreeMap<_, _>>();
    let actual_fact_removals = before_facts
        .keys()
        .filter(|id| !after_facts.contains_key(id))
        .copied()
        .collect::<BTreeSet<_>>();
    let delta_fact_removals = delta
        .removed_fact_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if actual_fact_removals != delta_fact_removals
        || delta_fact_removals.len() != delta.removed_fact_ids.len()
    {
        return Err("fact removals differ from the direct delta".to_owned());
    }
    let required_upserts = after_facts
        .iter()
        .filter_map(|(id, fact)| match before_facts.get(id) {
            None => Some(*id),
            Some(before_fact) if !fact_maps_without_change(before_fact, fact, edit) => Some(*id),
            Some(_) => None,
        })
        .collect::<BTreeSet<_>>();
    let delta_upserts = delta
        .upserted_facts
        .iter()
        .map(|fact| fact.id)
        .collect::<BTreeSet<_>>();
    if required_upserts != delta_upserts || delta_upserts.len() != delta.upserted_facts.len() {
        return Err(format!(
            "fact upserts differ: delta={delta_upserts:?} required={required_upserts:?}"
        ));
    }
    for fact in &delta.upserted_facts {
        if after_facts.get(&fact.id).copied() != Some(fact) {
            return Err(format!("upserted fact {:?} differs from snapshot", fact.id));
        }
    }

    for range in protected {
        for fact in before.facts.iter().filter(|fact| {
            range.old.start <= fact.source.start
                && fact.source.end <= range.old.end
                && (fact.source.end <= edit.start_utf8 || edit.end_utf8 <= fact.source.start)
        }) {
            let expected_source = map_unchanged_range(&fact.source, edit)?;
            let expected_markers = fact
                .markers
                .iter()
                .map(|marker| map_unchanged_range(marker, edit))
                .collect::<Result<Vec<_>, _>>()?;
            let Some(actual) = after_facts.get(&fact.id) else {
                return Err(format!("protected fact ID {:?} churned", fact.id));
            };
            if actual.kind != fact.kind
                || actual.parent != fact.parent
                || actual.source != expected_source
                || actual.markers != expected_markers
            {
                return Err(format!("protected fact ID {:?} changed", fact.id));
            }
            if delta_fact_removals.contains(&fact.id) || delta_upserts.contains(&fact.id) {
                return Err(format!("protected fact ID {:?} appeared in delta", fact.id));
            }
        }
    }

    let upsert_ids = delta_upserts;
    let mut replayed_facts = before
        .facts
        .iter()
        .filter(|fact| !delta_fact_removals.contains(&fact.id) && !upsert_ids.contains(&fact.id))
        .map(|fact| {
            Ok(SyntaxFact {
                id: fact.id,
                parent: fact.parent,
                kind: fact.kind.clone(),
                source: map_unchanged_range(&fact.source, edit)?,
                markers: fact
                    .markers
                    .iter()
                    .map(|marker| map_unchanged_range(marker, edit))
                    .collect::<Result<Vec<_>, _>>()?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    replayed_facts.extend(delta.upserted_facts.iter().cloned());
    let replayed_facts = replayed_facts
        .into_iter()
        .map(|fact| (fact.id, fact))
        .collect::<BTreeMap<_, _>>();
    let committed_facts = after
        .facts
        .iter()
        .cloned()
        .map(|fact| (fact.id, fact))
        .collect::<BTreeMap<_, _>>();
    if replayed_facts != committed_facts {
        return Err("fact delta does not replay to the committed snapshot".to_owned());
    }
    Ok(())
}

fn fact_maps_without_change(before: &SyntaxFact, after: &SyntaxFact, edit: &Edit) -> bool {
    if before.kind != after.kind || before.parent != after.parent {
        return false;
    }
    let Ok(source) = map_unchanged_range(&before.source, edit) else {
        return false;
    };
    let markers = before
        .markers
        .iter()
        .map(|range| map_unchanged_range(range, edit))
        .collect::<Result<Vec<_>, _>>();
    source == after.source && markers.as_deref() == Ok(after.markers.as_slice())
}

fn map_unchanged_range(range: &Range<usize>, edit: &Edit) -> Result<Range<usize>, String> {
    if range.end <= edit.start_utf8 {
        return Ok(range.clone());
    }
    if edit.end_utf8 <= range.start {
        let delta = edit.replacement.len() as isize - (edit.end_utf8 - edit.start_utf8) as isize;
        let start = range
            .start
            .checked_add_signed(delta)
            .ok_or_else(|| "range mapping underflow".to_owned())?;
        let end = range
            .end
            .checked_add_signed(delta)
            .ok_or_else(|| "range mapping underflow".to_owned())?;
        return Ok(start..end);
    }
    Err(format!("range {range:?} intersects edit"))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryReport {
    pub revisions: usize,
    pub polls: usize,
    pub pending_polls: usize,
    pub maximum_delta_bytes: usize,
    pub comrak_differences: usize,
    pub pulldown_differences: usize,
    pub peer_oracle_disagreements: usize,
}

pub fn run_history<E: GateAEngine>(
    history: &EditHistory,
    fuel: WorkFuel,
    caps: ResourceCaps,
) -> Result<HistoryReport, String> {
    let first = history
        .revisions
        .first()
        .ok_or_else(|| format!("{} has no revisions", history.name))?;
    let mut engine = E::open(history.profile, &first.source).map_err(engine_error)?;
    let initial = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&first.source, &initial)?;
    validate_fact_probes(&first.source, &initial, &first.facts)?;
    let initial_clean = E::clean_snapshot(history.profile, &first.source).map_err(engine_error)?;
    validate_clean_equivalence(&initial, &initial_clean)?;
    validate_resources(&initial, None, caps)?;

    let mut report = HistoryReport {
        revisions: 1,
        ..HistoryReport::default()
    };
    record_differential_evidence(&first.source, &initial, &mut report)?;
    let mut previous = first.source.clone();
    for expected in history.revisions.iter().skip(1) {
        let edit = minimal_edit(engine.revision(), &previous, &expected.source);
        let applied = edit.apply(&previous)?;
        if applied != expected.source {
            return Err(format!("{} generated an invalid edit", history.name));
        }
        let begin = engine.begin_edit(edit).map_err(engine_error)?;
        validate_non_poll_phase(begin, "begin_edit")?;
        let mut polls_this_revision = 0;
        loop {
            let receipt = engine.poll(fuel).map_err(engine_error)?;
            validate_poll(receipt, fuel)?;
            report.polls += 1;
            polls_this_revision += 1;
            if receipt.status == PollStatus::Pending {
                report.pending_polls += 1;
                if polls_this_revision > 1_000_000 {
                    return Err("edit did not converge within one million polls".to_owned());
                }
                continue;
            }
            if receipt.status != PollStatus::Ready {
                return Err(format!("unexpected poll status {:?}", receipt.status));
            }
            break;
        }
        let (delta, commit) = engine.commit().map_err(engine_error)?;
        validate_non_poll_phase(commit, "commit")?;
        let source = engine.materialize_source();
        if source != expected.source {
            return Err(format!("{} committed wrong source", history.name));
        }
        let snapshot = engine.snapshot().map_err(engine_error)?;
        validate_snapshot(&source, &snapshot)?;
        record_differential_evidence(&source, &snapshot, &mut report)?;
        validate_fact_probes(&source, &snapshot, &expected.facts)?;
        let clean = E::clean_snapshot(history.profile, &source).map_err(engine_error)?;
        validate_clean_equivalence(&snapshot, &clean)?;
        validate_resources(&snapshot, Some(&delta), caps)?;
        report.maximum_delta_bytes = report.maximum_delta_bytes.max(delta.encoded_bytes);
        report.revisions += 1;
        previous = source;
    }
    if history.require_pending && report.pending_polls == 0 {
        return Err(format!("{} never yielded", history.name));
    }
    Ok(report)
}

fn record_differential_evidence(
    source: &str,
    snapshot: &Snapshot,
    report: &mut HistoryReport,
) -> Result<(), String> {
    let comrak = oracle_html_for(snapshot.profile, source)?;
    let pulldown = pulldown_oracle_html_for(snapshot.profile, source);
    report.comrak_differences += usize::from(snapshot.normalized_html != comrak);
    report.pulldown_differences += usize::from(snapshot.normalized_html != pulldown);
    report.peer_oracle_disagreements += usize::from(comrak != pulldown);
    Ok(())
}

fn engine_error(error: impl Display) -> String {
    error.to_string()
}

pub fn hash64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GiantLineCase {
    pub name: &'static str,
    pub source: String,
    pub edit: Edit,
    pub fuel: WorkFuel,
}

pub fn giant_line_cases(bytes: usize) -> Vec<GiantLineCase> {
    assert!(bytes >= 1024);
    let payload = "a".repeat(bytes);
    let paragraph = format!("{payload}\n\nafter\n");
    let comment = format!("<!--{payload}-->\n\nafter\n");
    let table = format!("| head | value |\n| --- | --- |\n| {payload} | x |\n\nafter\n");
    [
        ("giant-paragraph", paragraph),
        ("giant-html-comment", comment),
        ("giant-table-row", table),
    ]
    .into_iter()
    .map(|(name, source)| {
        let middle = source.find(&payload).unwrap() + payload.len() / 2;
        GiantLineCase {
            name,
            edit: Edit {
                base_revision: 0,
                start_utf8: middle,
                end_utf8: middle + 1,
                replacement: "Z".to_owned(),
            },
            source,
            fuel: WorkFuel::new(4 * 1024, 4 * 1024),
        }
    })
    .collect()
}

/// Cold insertion forces the candidate to create sub-line continuation state:
/// none of the inserted bytes existed at an old checkpoint. These histories
/// must therefore produce at least one Pending poll under 4 KiB fuel.
pub fn cold_giant_line_histories(bytes: usize) -> Vec<EditHistory> {
    assert!(bytes >= 1024);
    let payload = "a".repeat(bytes);
    [
        ("cold-giant-paragraph", format!("{payload}\n")),
        ("cold-giant-html-comment", format!("<!--{payload}-->\n")),
        (
            "cold-giant-table-row",
            format!("| head | value |\n| --- | --- |\n| {payload} | x |\n"),
        ),
    ]
    .into_iter()
    .map(|(name, inserted)| EditHistory {
        name,
        profile: SyntaxProfile::FlarkGfm,
        revisions: vec![
            RevisionExpectation::source("before\n\nafter\n"),
            RevisionExpectation::source(format!("before\n\n{inserted}\nafter\n")),
        ],
        require_pending: true,
    })
    .collect()
}

pub fn dense_line_source(lines: usize) -> String {
    "a\n".repeat(lines)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalityCase {
    pub name: &'static str,
    pub source: String,
    pub edit: Edit,
    pub protected: Vec<ProtectedRange>,
}

pub fn giant_construct_locality_cases() -> Vec<LocalityCase> {
    const PREFIX: &str = "# protected-prefix\n\n";
    const SUFFIX: &str = "\n# protected-suffix\n";

    let mut list = String::new();
    for index in 0..70_000 {
        list.push_str(&format!("- item-{index:05}\n"));
    }

    let mut table = String::from("| left | right |\n| --- | --- |\n");
    for index in 0..45_000 {
        table.push_str(&format!("| cell-{index:05} | value |\n"));
    }

    let html = format!("<!--\n{}\n-->\n", "payload\n".repeat(131_072));

    [
        ("giant-list-local-delta", list, "item-35000", "EDIT-35000"),
        ("giant-table-local-delta", table, "cell-22500", "EDIT-22500"),
        (
            "giant-html-local-delta",
            html,
            "payload\npayload\npayload",
            "payload\nEDITED!\npayload",
        ),
    ]
    .into_iter()
    .map(|(name, body, needle, replacement)| {
        let source = format!("{PREFIX}{body}{SUFFIX}");
        let start = source.find(needle).unwrap();
        let suffix_start = source.len() - SUFFIX.len();
        LocalityCase {
            name,
            source,
            edit: Edit {
                base_revision: 0,
                start_utf8: start,
                end_utf8: start + needle.len(),
                replacement: replacement.to_owned(),
            },
            protected: vec![
                ProtectedRange {
                    old: 0..PREFIX.len(),
                },
                ProtectedRange {
                    old: suffix_start..suffix_start + SUFFIX.len(),
                },
            ],
        }
    })
    .collect()
}

pub fn run_locality_case<E: GateAEngine>(
    case: &LocalityCase,
    fuel: WorkFuel,
    caps: ResourceCaps,
) -> Result<HistoryReport, String> {
    let mut engine = E::open(SyntaxProfile::FlarkGfm, &case.source).map_err(engine_error)?;
    let before = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&case.source, &before)?;
    let before_clean =
        E::clean_snapshot(SyntaxProfile::FlarkGfm, &case.source).map_err(engine_error)?;
    validate_clean_equivalence(&before, &before_clean)?;
    validate_resources(&before, None, caps)?;

    let mut edit = case.edit.clone();
    edit.base_revision = engine.revision();
    let expected = edit.apply(&case.source)?;
    let begin = engine.begin_edit(edit.clone()).map_err(engine_error)?;
    validate_non_poll_phase(begin, "begin_edit")?;
    let mut report = HistoryReport {
        revisions: 1,
        ..HistoryReport::default()
    };
    record_differential_evidence(&case.source, &before, &mut report)?;
    loop {
        let receipt = engine.poll(fuel).map_err(engine_error)?;
        validate_poll(receipt, fuel)?;
        report.polls += 1;
        match receipt.status {
            PollStatus::Pending if report.polls <= 1_000_000 => {
                report.pending_polls += 1;
            }
            PollStatus::Pending => {
                return Err(format!("{} did not converge", case.name));
            }
            PollStatus::Ready => break,
            PollStatus::Cancelled => {
                return Err(format!("{} was unexpectedly cancelled", case.name));
            }
        }
    }
    let (delta, commit) = engine.commit().map_err(engine_error)?;
    validate_non_poll_phase(commit, "commit")?;
    validate_delta_receipt(&delta, caps)?;
    if engine.materialize_source() != expected {
        return Err(format!("{} committed wrong source", case.name));
    }
    let after = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(&expected, &after)?;
    record_differential_evidence(&expected, &after, &mut report)?;
    let clean = E::clean_snapshot(SyntaxProfile::FlarkGfm, &expected).map_err(engine_error)?;
    validate_clean_equivalence(&after, &clean)?;
    validate_resources(&after, Some(&delta), caps)?;
    validate_local_delta(&before, &after, &edit, &delta, &case.protected)?;
    report.maximum_delta_bytes = delta.encoded_bytes;
    report.revisions = 2;
    Ok(report)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryGateReport {
    pub dense_lines: ResourceMetrics,
    pub byte_heavy: ResourceMetrics,
}

pub fn run_memory_gate<E: GateAEngine>(caps: ResourceCaps) -> Result<MemoryGateReport, String> {
    let dense = dense_line_source(1_000_000);
    let mut byte_heavy = String::with_capacity(10 * 1024 * 1024);
    while byte_heavy.len() < 10 * 1024 * 1024 {
        byte_heavy.push_str("- ordinary item with enough text to exercise source chunks\n");
    }
    let dense_lines = check_memory_fixture::<E>(&dense, caps.strictest(ResourceCaps::DENSE_LINES))?;
    let byte_heavy = check_memory_fixture::<E>(&byte_heavy, caps)?;
    Ok(MemoryGateReport {
        dense_lines,
        byte_heavy,
    })
}

fn check_memory_fixture<E: GateAEngine>(
    source: &str,
    caps: ResourceCaps,
) -> Result<ResourceMetrics, String> {
    let engine = E::open(SyntaxProfile::FlarkGfm, source).map_err(engine_error)?;
    if engine.materialize_source() != source {
        return Err("memory fixture source mismatch".to_owned());
    }
    let resources = engine.resource_metrics();
    validate_resource_metrics(&resources, caps)?;
    Ok(resources)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrderOperation {
    Insert { slot: usize, serial: u64 },
    Remove { slot: usize },
}

/// Deterministic 100k-operation history with bounded live source. The target
/// is sequence-index durability, not random-number quality.
pub fn randomized_order_operations(count: usize, max_live_items: usize) -> Vec<OrderOperation> {
    assert!(max_live_items > 1);
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut live = 0usize;
    let mut operations = Vec::with_capacity(count);
    for serial in 0..count as u64 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let insert = live == 0 || (live < max_live_items && state & 3 != 0);
        if insert {
            let slot = state as usize % (live + 1);
            operations.push(OrderOperation::Insert { slot, serial });
            live += 1;
        } else {
            let slot = state as usize % live;
            operations.push(OrderOperation::Remove { slot });
            live -= 1;
        }
    }
    operations
}

pub fn same_boundary_insertions(count: usize) -> Vec<OrderOperation> {
    (0..count as u64)
        .map(|serial| OrderOperation::Insert { slot: 0, serial })
        .collect()
}

/// Edits that activate a genuinely global block reclassification. These must
/// not be mistaken for a locality failure: the syntax blast radius is real,
/// but its sequence invalidation/delta still has to stay compact and preserve
/// identity outside the affected range.
pub fn global_reclassification_cases() -> Vec<LocalityCase> {
    const PREFIX: &str = "# protected-prefix\n\n";
    const SUFFIX: &str = "\n# protected-suffix\n";
    let mut body = String::new();
    for index in 0..60_000 {
        body.push_str(&format!("ordinary paragraph {index:05}\n\n"));
    }
    body.push_str("-->\n");
    let source = format!("{PREFIX}{body}{SUFFIX}");
    let suffix_start = source.len() - SUFFIX.len();
    vec![LocalityCase {
        name: "activate-global-html-comment",
        source,
        edit: Edit {
            base_revision: 0,
            start_utf8: PREFIX.len(),
            end_utf8: PREFIX.len(),
            replacement: "<!--\n".to_owned(),
        },
        protected: vec![
            ProtectedRange {
                old: 0..PREFIX.len(),
            },
            ProtectedRange {
                old: suffix_start..suffix_start + SUFFIX.len(),
            },
        ],
    }]
}

pub fn apply_order_operation(items: &mut Vec<String>, operation: &OrderOperation) -> Edit {
    let source = order_source(items);
    let edit = match operation {
        OrderOperation::Insert { slot, serial } => {
            let start = order_slot_offset(items, *slot);
            let value = format!("- item-{serial:06}\n");
            items.insert(*slot, value.clone());
            Edit {
                base_revision: 0,
                start_utf8: start,
                end_utf8: start,
                replacement: value,
            }
        }
        OrderOperation::Remove { slot } => {
            let start = order_slot_offset(items, *slot);
            let removed = items.remove(*slot);
            Edit {
                base_revision: 0,
                start_utf8: start,
                end_utf8: start + removed.len(),
                replacement: String::new(),
            }
        }
    };
    debug_assert_eq!(edit.apply(&source).unwrap(), order_source(items));
    edit
}

pub fn order_source(items: &[String]) -> String {
    let mut source = String::from("prefix\n\n");
    for item in items {
        source.push_str(item);
    }
    source.push_str("\nsuffix\n");
    source
}

fn order_slot_offset(items: &[String], slot: usize) -> usize {
    assert!(slot <= items.len());
    "prefix\n\n".len() + items.iter().take(slot).map(String::len).sum::<usize>()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrderGateReport {
    pub operations: usize,
    pub polls: usize,
    pub oracle_checkpoints: usize,
    pub maximum_delta_bytes: usize,
}

pub fn run_stable_order_gate<E: GateAEngine>(
    fuel: WorkFuel,
    caps: ResourceCaps,
    oracle_stride: usize,
) -> Result<(OrderGateReport, OrderGateReport), String> {
    if oracle_stride == 0 {
        return Err("oracle stride must be non-zero".to_owned());
    }
    let same = same_boundary_insertions(10_000);
    let random = randomized_order_operations(100_000, 256);
    Ok((
        run_order_plan::<E>(&same, fuel, caps, oracle_stride)?,
        run_order_plan::<E>(&random, fuel, caps, oracle_stride)?,
    ))
}

pub fn run_order_plan<E: GateAEngine>(
    operations: &[OrderOperation],
    fuel: WorkFuel,
    caps: ResourceCaps,
    oracle_stride: usize,
) -> Result<OrderGateReport, String> {
    if oracle_stride == 0 {
        return Err("oracle stride must be non-zero".to_owned());
    }
    let mut items = Vec::new();
    let mut source = order_source(&items);
    let mut engine = E::open(SyntaxProfile::FlarkGfm, &source).map_err(engine_error)?;
    check_order_snapshot::<E>(&engine, &source, caps)?;
    let mut report = OrderGateReport {
        oracle_checkpoints: 1,
        ..OrderGateReport::default()
    };

    for (index, operation) in operations.iter().enumerate() {
        let mut edit = apply_order_operation(&mut items, operation);
        edit.base_revision = engine.revision();
        let expected = edit.apply(&source)?;
        let begin = engine.begin_edit(edit).map_err(engine_error)?;
        validate_non_poll_phase(begin, "begin_edit")?;
        let mut polls = 0usize;
        loop {
            let receipt = engine.poll(fuel).map_err(engine_error)?;
            validate_poll(receipt, fuel)?;
            report.polls += 1;
            polls += 1;
            match receipt.status {
                PollStatus::Pending if polls <= 1_000_000 => continue,
                PollStatus::Pending => {
                    return Err(format!("order operation {index} did not converge"));
                }
                PollStatus::Ready => break,
                PollStatus::Cancelled => {
                    return Err(format!("order operation {index} was cancelled"));
                }
            }
        }
        let (delta, commit) = engine.commit().map_err(engine_error)?;
        validate_non_poll_phase(commit, "commit")?;
        if delta.base_revision + 1 != delta.revision || delta.revision != engine.revision() {
            return Err(format!(
                "order operation {index} produced invalid revisions"
            ));
        }
        validate_delta_receipt(&delta, caps)?;
        report.maximum_delta_bytes = report.maximum_delta_bytes.max(delta.encoded_bytes);
        report.operations += 1;
        source = expected;

        if (index + 1) % oracle_stride == 0 || index + 1 == operations.len() {
            check_order_snapshot::<E>(&engine, &source, caps)?;
            report.oracle_checkpoints += 1;
        }
    }
    Ok(report)
}

fn check_order_snapshot<E: GateAEngine>(
    engine: &E,
    source: &str,
    caps: ResourceCaps,
) -> Result<(), String> {
    if engine.materialize_source() != source {
        return Err("stable-order history committed wrong source".to_owned());
    }
    let snapshot = engine.snapshot().map_err(engine_error)?;
    validate_snapshot(source, &snapshot)?;
    validate_resources(&snapshot, None, caps)?;
    let clean = E::clean_snapshot(SyntaxProfile::FlarkGfm, source).map_err(engine_error)?;
    validate_clean_equivalence(&snapshot, &clean)
}
