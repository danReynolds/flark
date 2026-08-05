//! Research-only bounded-Comrak architecture challenger.
//!
//! The important ordering is intentional: a source-backed block machine first
//! closes a semantic region and measures its range; only then may stock Comrak
//! see the region. An oversized region therefore creates no Comrak arena,
//! `Ast.content`, `Ast.line_offsets`, delimiter stack, or inline AST at all.
//!
//! This proves an allocation boundary for the block subset implemented by the
//! companion Comrak-derived probe. It does *not* prove a grammar-exact front
//! pass for CommonMark/GFM. Unsupported or document-global syntax is marked
//! opaque rather than guessed, because guessing would recreate dual parsing.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use comrak::nodes::NodeValue;
use comrak::{markdown_to_html, parse_document, Arena, Options};
use flark_comrak_derived_core_probe::{ContainerKind, DerivedBlockMachine, LineKind, LineRecord};

const POLL_FUEL: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionShape {
    Paragraph,
    SetextHeading,
    FencedCode,
    List,
    BlockQuote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedRisk {
    /// GFM tables need header promotion and row/cell continuation state.
    Table,
    /// HTML block classes have grammar-specific terminators, often without a
    /// blank line before later Markdown.
    HtmlBlock,
    /// Definitions and uses have document-global first-definition-wins state.
    ReferenceOrFootnote,
    /// ATX headings, indented code, thematic breaks, and enabled extensions are
    /// outside the current research spine.
    OtherBlockGrammar,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegionDisposition {
    Delegated {
        ast_nodes: usize,
        inline_nodes: usize,
    },
    /// The block spine knows this is an inert raw block, so no inline parse is
    /// required regardless of payload size. Rendering can stay source-backed.
    SourceBackedRaw,
    OpaqueOverCap,
    OpaqueUnsupported(UnsupportedRisk),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub range: Range<usize>,
    pub shape: RegionShape,
    pub disposition: RegionDisposition,
    pub source_fingerprint: u64,
}

impl Region {
    pub fn len(&self) -> usize {
        self.range.len()
    }

    pub fn is_empty(&self) -> bool {
        self.range.is_empty()
    }

    pub fn is_delegated(&self) -> bool {
        matches!(self.disposition, RegionDisposition::Delegated { .. })
    }

    pub fn has_live_semantics(&self) -> bool {
        matches!(
            self.disposition,
            RegionDisposition::Delegated { .. } | RegionDisposition::SourceBackedRaw
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanMetrics {
    pub block_work_units: usize,
    pub source_bytes_inspected: usize,
    pub maximum_bytes_per_poll: usize,
    pub completed_lines: usize,
    pub drained_line_records: usize,
    pub delegated_regions: usize,
    pub delegated_source_bytes: usize,
    pub delegated_ast_nodes: usize,
    pub opaque_regions: usize,
    pub opaque_source_bytes: usize,
    pub source_backed_raw_regions: usize,
    pub source_backed_raw_bytes: usize,
    /// Calls made before the block spine had closed and measured a region.
    /// This must remain zero; a non-zero value invalidates the experiment.
    pub premature_comrak_calls: usize,
}

#[derive(Clone, Debug)]
pub struct BoundedIndex {
    source: Arc<str>,
    pub cap_bytes: usize,
    pub regions: Vec<Region>,
    pub metrics: ScanMetrics,
}

impl BoundedIndex {
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn delegated_html(&self, options: &Options<'_>) -> String {
        let mut html = String::new();
        for region in &self.regions {
            match region.disposition {
                RegionDisposition::Delegated { .. } => {
                    html.push_str(&markdown_to_html(
                        &self.source[region.range.clone()],
                        options,
                    ));
                }
                RegionDisposition::SourceBackedRaw
                | RegionDisposition::OpaqueOverCap
                | RegionDisposition::OpaqueUnsupported(_) => {
                    html.push_str("<flark-opaque-region></flark-opaque-region>\n");
                }
            }
        }
        html
    }

    pub fn region_containing(&self, offset: usize) -> Option<&Region> {
        self.regions
            .iter()
            .find(|region| region.range.start <= offset && offset < region.range.end)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegionKey {
    Container(u64),
    Leaf(u64),
}

#[derive(Clone, Debug)]
struct PendingRegion {
    key: RegionKey,
    range: Range<usize>,
    shape: RegionShape,
    risk: Option<UnsupportedRisk>,
    content_ranges: Vec<Range<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Grouping {
    /// Useful as a falsifier: delegating a whole top-level list makes a large
    /// list opaque even when every inline leaf is tiny.
    TopLevelEnvelope,
    /// Proposed hybrid: the definitive spine owns containers/tightness while
    /// stock Comrak is exposed only as a bounded inline-leaf service.
    InlineLeaf,
}

/// Scan source into bounded semantic regions, delegating only after closure.
///
/// The returned index is exact only for the deliberately small block subset of
/// `flark-comrak-derived-core-probe`. Every known unsupported construct is
/// fail-closed, but the detector itself is not a complete grammar and must not
/// be presented as a production authority.
pub fn build_bounded_index(
    source: impl Into<Arc<str>>,
    cap_bytes: usize,
    options: &Options<'_>,
) -> BoundedIndex {
    build_index(
        source.into(),
        cap_bytes,
        options,
        Grouping::TopLevelEnvelope,
    )
}

/// Build the more promising hybrid shape: block structure remains in the
/// definitive spine, while each paragraph/heading/item leaf is handed to a
/// bounded Comrak inline service. The current implementation uses a stock
/// full-document parse of a bounded logical leaf as a stand-in because Comrak
/// does not expose its inline `Subject`; production would require that hook.
pub fn build_bounded_leaf_index(
    source: impl Into<Arc<str>>,
    cap_bytes: usize,
    options: &Options<'_>,
) -> BoundedIndex {
    build_index(source.into(), cap_bytes, options, Grouping::InlineLeaf)
}

fn build_index(
    source: Arc<str>,
    cap_bytes: usize,
    options: &Options<'_>,
    grouping: Grouping,
) -> BoundedIndex {
    assert!(cap_bytes > 0);
    let mut machine = DerivedBlockMachine::new(Arc::clone(&source));
    let mut metrics = ScanMetrics::default();
    let mut pending: Option<PendingRegion> = None;
    let mut regions = Vec::new();

    while !machine.is_complete() {
        let report = machine.advance(POLL_FUEL);
        metrics.block_work_units += report.work_units;
        metrics.source_bytes_inspected += report.bytes_inspected;
        metrics.maximum_bytes_per_poll = metrics.maximum_bytes_per_poll.max(report.bytes_inspected);
        metrics.completed_lines += report.completed_lines;

        for record in machine.take_records() {
            metrics.drained_line_records += 1;
            consume_record(
                &source,
                record,
                &mut pending,
                &mut regions,
                &mut metrics,
                cap_bytes,
                options,
                grouping,
            );
        }
    }

    if let Some(region) = pending.take() {
        close_region(
            &source,
            region,
            &mut regions,
            &mut metrics,
            cap_bytes,
            options,
            grouping,
        );
    }

    BoundedIndex {
        source,
        cap_bytes,
        regions,
        metrics,
    }
}

#[allow(clippy::too_many_arguments)]
fn consume_record(
    source: &str,
    record: LineRecord,
    pending: &mut Option<PendingRegion>,
    regions: &mut Vec<Region>,
    metrics: &mut ScanMetrics,
    cap_bytes: usize,
    options: &Options<'_>,
    grouping: Grouping,
) {
    let key = record_key(&record, grouping);
    let Some(key) = key else {
        if record.chunk.container_path().is_empty() {
            if let Some(region) = pending.take() {
                close_region(
                    source, region, regions, metrics, cap_bytes, options, grouping,
                );
            }
        } else if let Some(region) = pending.as_mut() {
            region.range.end = region.range.end.max(record.chunk.source.end);
        }
        return;
    };

    if pending.as_ref().is_some_and(|region| region.key != key) {
        close_region(
            source,
            pending.take().expect("pending region exists"),
            regions,
            metrics,
            cap_bytes,
            options,
            grouping,
        );
    }

    let line_shape = shape_for_record(&record);
    let line_risk = unsupported_risk(source, &record);
    let region = pending.get_or_insert_with(|| PendingRegion {
        key,
        range: record.chunk.source.clone(),
        shape: line_shape,
        risk: line_risk,
        content_ranges: Vec::new(),
    });
    region.range.end = region.range.end.max(record.chunk.source.end);
    region.shape = merge_shape(region.shape, line_shape);
    if region.risk.is_none() {
        region.risk = line_risk;
    }
    if grouping == Grouping::InlineLeaf {
        if region.range.len() > cap_bytes || region.shape == RegionShape::FencedCode {
            // Never retain one range per line in an oversized or inert raw
            // leaf. Crossing the cap releases even the bounded prefix vector.
            region.content_ranges = Vec::new();
        } else if !matches!(
            record.chunk.kind,
            LineKind::SetextUnderline | LineKind::Blank
        ) && !record.chunk.content.is_empty()
        {
            region.content_ranges.push(record.chunk.content);
        }
    }
}

fn record_key(record: &LineRecord, grouping: Grouping) -> Option<RegionKey> {
    match grouping {
        Grouping::TopLevelEnvelope => {
            if let Some(frame) = record.chunk.container_path().first() {
                Some(RegionKey::Container(frame.id))
            } else {
                record.chunk.leaf_id.map(RegionKey::Leaf)
            }
        }
        Grouping::InlineLeaf => record.chunk.leaf_id.map(RegionKey::Leaf),
    }
}

fn shape_for_record(record: &LineRecord) -> RegionShape {
    if let Some(frame) = record.chunk.container_path().first() {
        return match frame.kind {
            ContainerKind::List(_) | ContainerKind::Item(_) => RegionShape::List,
            ContainerKind::BlockQuote => RegionShape::BlockQuote,
        };
    }
    match record.chunk.kind {
        LineKind::SetextUnderline => RegionShape::SetextHeading,
        LineKind::FenceOpen | LineKind::FenceBody | LineKind::FenceClose => RegionShape::FencedCode,
        LineKind::Blank | LineKind::Paragraph => RegionShape::Paragraph,
    }
}

fn merge_shape(existing: RegionShape, next: RegionShape) -> RegionShape {
    use RegionShape::{BlockQuote, FencedCode, List, Paragraph, SetextHeading};
    match (existing, next) {
        (List, _) | (_, List) => List,
        (BlockQuote, _) | (_, BlockQuote) => BlockQuote,
        (FencedCode, _) | (_, FencedCode) => FencedCode,
        (SetextHeading, _) | (_, SetextHeading) => SetextHeading,
        (Paragraph, Paragraph) => Paragraph,
    }
}

fn unsupported_risk(source: &str, record: &LineRecord) -> Option<UnsupportedRisk> {
    if record.chunk.kind == LineKind::SetextUnderline {
        return None;
    }
    let line = &source[record.chunk.source.clone()];
    let without_eol = line.trim_end_matches(['\r', '\n']);
    let trimmed = without_eol.trim_start_matches(' ');
    let indent = without_eol.len() - trimmed.len();

    if trimmed.starts_with('<') {
        return Some(UnsupportedRisk::HtmlBlock);
    }
    if trimmed.contains('|') {
        return Some(UnsupportedRisk::Table);
    }
    // This is deliberately conservative. Inline links could be local, but a
    // complete distinction from reference/footnote syntax belongs in the
    // authoritative inline scanner, not this front pass.
    if trimmed.contains('[') || trimmed.contains(']') {
        return Some(UnsupportedRisk::ReferenceOrFootnote);
    }
    if indent >= 4
        || trimmed.starts_with('#')
        || trimmed.starts_with(":::")
        || looks_like_thematic_break(trimmed)
    {
        return Some(UnsupportedRisk::OtherBlockGrammar);
    }
    None
}

fn looks_like_thematic_break(line: &str) -> bool {
    let mut marker = None;
    let mut count = 0usize;
    for byte in line.bytes() {
        if byte == b' ' || byte == b'\t' {
            continue;
        }
        if !matches!(byte, b'*' | b'-' | b'_') {
            return false;
        }
        match marker {
            None => marker = Some(byte),
            Some(current) if current == byte => {}
            Some(_) => return false,
        }
        count += 1;
    }
    count >= 3
}

fn close_region(
    source: &str,
    pending: PendingRegion,
    regions: &mut Vec<Region>,
    metrics: &mut ScanMetrics,
    cap_bytes: usize,
    options: &Options<'_>,
    grouping: Grouping,
) {
    let fragment = &source[pending.range.clone()];
    let source_fingerprint = fingerprint(fragment);
    let disposition =
        if grouping == Grouping::InlineLeaf && pending.shape == RegionShape::FencedCode {
            metrics.source_backed_raw_regions += 1;
            metrics.source_backed_raw_bytes += fragment.len();
            RegionDisposition::SourceBackedRaw
        } else if let Some(risk) = pending.risk {
            metrics.opaque_regions += 1;
            metrics.opaque_source_bytes += fragment.len();
            RegionDisposition::OpaqueUnsupported(risk)
        } else if fragment.len() > cap_bytes {
            metrics.opaque_regions += 1;
            metrics.opaque_source_bytes += fragment.len();
            RegionDisposition::OpaqueOverCap
        } else {
            // This is the first point at which Comrak is invoked. The complete
            // semantic envelope is already closed and measured.
            let logical_leaf;
            let delegated_source = if grouping == Grouping::InlineLeaf {
                logical_leaf = logical_leaf_source(source, &pending.content_ranges);
                logical_leaf.as_str()
            } else {
                fragment
            };
            let arena = Arena::new();
            let root = parse_document(&arena, delegated_source, options);
            let mut ast_nodes = 0usize;
            let mut inline_nodes = 0usize;
            for node in root.descendants() {
                ast_nodes += 1;
                if !node.data().value.block() {
                    inline_nodes += 1;
                }
            }
            metrics.delegated_regions += 1;
            metrics.delegated_source_bytes += delegated_source.len();
            metrics.delegated_ast_nodes += ast_nodes;
            RegionDisposition::Delegated {
                ast_nodes,
                inline_nodes,
            }
        };

    regions.push(Region {
        range: pending.range,
        shape: pending.shape,
        disposition,
        source_fingerprint,
    });
}

fn logical_leaf_source(source: &str, ranges: &[Range<usize>]) -> String {
    let capacity = ranges.iter().map(Range::len).sum::<usize>() + ranges.len();
    let mut logical = String::with_capacity(capacity);
    for (index, range) in ranges.iter().enumerate() {
        if index > 0 {
            logical.push('\n');
        }
        logical.push_str(&source[range.clone()]);
    }
    logical
}

fn fingerprint(source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionDelta {
    pub common_prefix: usize,
    pub common_suffix: usize,
    pub removed_regions: usize,
    pub inserted_regions: usize,
    pub changed_source_bytes: usize,
    /// A compact range/identity/disposition delta, excluding source bytes the
    /// editor already owns. This is an estimate, not a wire-format claim.
    pub estimated_protocol_bytes: usize,
}

pub fn diff_regions(before: &[Region], after: &[Region]) -> RegionDelta {
    let mut prefix = 0usize;
    while prefix < before.len()
        && prefix < after.len()
        && same_region(&before[prefix], &after[prefix])
    {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < before.len().saturating_sub(prefix)
        && suffix < after.len().saturating_sub(prefix)
        && same_region(
            &before[before.len() - 1 - suffix],
            &after[after.len() - 1 - suffix],
        )
    {
        suffix += 1;
    }
    let removed_regions = before.len().saturating_sub(prefix + suffix);
    let inserted_regions = after.len().saturating_sub(prefix + suffix);
    let changed_source_bytes = after[prefix..after.len().saturating_sub(suffix)]
        .iter()
        .map(Region::len)
        .sum();
    RegionDelta {
        common_prefix: prefix,
        common_suffix: suffix,
        removed_regions,
        inserted_regions,
        changed_source_bytes,
        estimated_protocol_bytes: 32 + inserted_regions * 48,
    }
}

fn same_region(left: &Region, right: &Region) -> bool {
    left.source_fingerprint == right.source_fingerprint
        && left.shape == right.shape
        && left.disposition == right.disposition
}

/// Return whether a same-length ASCII replacement is certified not to change
/// the *supported subset's* block envelope. This permits an opaque region to
/// remain opaque without rescanning its entire payload. It is intentionally
/// narrow and is not a general Markdown edit classifier.
pub fn is_certified_payload_edit(source: &str, range: Range<usize>, replacement: &str) -> bool {
    if range.len() != replacement.len()
        || !replacement.is_ascii()
        || replacement.bytes().any(|byte| {
            matches!(
                byte,
                b'\r' | b'\n' | b'`' | b'~' | b'<' | b'>' | b'|' | b'[' | b']'
            )
        })
    {
        return false;
    }
    let Some(original) = source.get(range.clone()) else {
        return false;
    };
    if !original.is_ascii()
        || original.bytes().any(|byte| {
            matches!(
                byte,
                b'\r' | b'\n' | b'`' | b'~' | b'<' | b'>' | b'|' | b'[' | b']'
            )
        })
    {
        return false;
    }
    // Up to three leading spaces affect block recognition. Refuse changes in
    // the first four bytes of any physical line even when the bytes look plain.
    let line_start = source[..range.start]
        .rfind(['\r', '\n'])
        .map_or(0, |offset| offset + 1);
    range.start.saturating_sub(line_start) >= 4
}

/// Render fragments independently without the fail-closed checks. This is a
/// test oracle for demonstrating why naive leaf-local delegation is wrong for
/// global references and footnotes; production code must not use it.
pub fn naive_fragment_html(source: &str, ranges: &[Range<usize>], options: &Options<'_>) -> String {
    let mut html = String::new();
    for range in ranges {
        html.push_str(&markdown_to_html(&source[range.clone()], options));
    }
    html
}

pub fn gfm_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.autolink = true;
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.tagfilter = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options
}

pub fn node_kind_counts(source: &str, options: &Options<'_>) -> (usize, usize) {
    let arena = Arena::new();
    let root = parse_document(&arena, source, options);
    let mut block = 0usize;
    let mut inline = 0usize;
    for node in root.descendants() {
        if !node.data().value.block() {
            inline += 1;
        } else if !matches!(node.data().value, NodeValue::Document) {
            block += 1;
        }
    }
    (block, inline)
}
