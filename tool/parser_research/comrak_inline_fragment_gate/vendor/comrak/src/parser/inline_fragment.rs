//! A hard-bounded, inline-only parser service for a certified block leaf.
//!
//! This module reuses Comrak's private inline parser with narrowly scoped,
//! annotation-only instrumentation. It does not invoke the block parser. A
//! caller-owned block spine supplies the logical inline content, an exact
//! logical-byte-boundary to physical-source map, and the document's
//! first-definition-wins reference snapshot.

#![allow(missing_copy_implementations, missing_docs)]

use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::mem;
use std::ops::Range;
use std::panic::RefUnwindSafe;

use crate::Arena;
use crate::nodes::{Ast, NodeHeading, NodeValue, Sourcepos};
use crate::parser::autolink;
use crate::parser::inlines::{
    FootnoteDefs, InlineAnnotation, InlineAnnotationKind,
    InlineReferenceResolution as ParserInlineReferenceResolution, InlineReferenceResolver, RefMap,
    Subject,
};
use crate::strings;

use super::{Options, ResolvedReference, Spx};

/// Candidate urgent-path ceiling for one inline-bearing logical leaf. Device
/// and corpus calibration may move this value or add a larger worker-only
/// exact path; over-cap source must remain visible rather than misparsed.
#[cfg(not(feature = "flark-inline-research"))]
pub const MAX_INLINE_FRAGMENT_BYTES: usize = 8 * 1024;
#[cfg(feature = "flark-inline-research")]
pub const MAX_INLINE_FRAGMENT_BYTES: usize = 64 * 1024;

/// Independent protocol ceilings. The input cap bounds parser work; these
/// guard the compact representation if its shape changes in a future Comrak
/// update.
#[cfg(not(feature = "flark-inline-research"))]
pub const MAX_INLINE_FACTS: usize = 16 * 1024;
#[cfg(feature = "flark-inline-research")]
pub const MAX_INLINE_FACTS: usize = 128 * 1024;
#[cfg(not(feature = "flark-inline-research"))]
pub const MAX_INLINE_PAYLOAD_BYTES: usize = 256 * 1024;
#[cfg(feature = "flark-inline-research")]
pub const MAX_INLINE_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
#[cfg(not(feature = "flark-inline-research"))]
pub const MAX_REFERENCE_DEPENDENCIES: usize = 2 * 1024;
#[cfg(feature = "flark-inline-research")]
pub const MAX_REFERENCE_DEPENDENCIES: usize = 16 * 1024;
#[cfg(not(feature = "flark-inline-research"))]
pub const MAX_INLINE_OUTPUT_BYTES: usize = 512 * 1024;
#[cfg(feature = "flark-inline-research")]
pub const MAX_INLINE_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

/// The grammar profile used by both the inline service and the block spine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineProfile {
    /// CommonMark 0.31.2 inline syntax.
    CommonMark,
    /// GitHub Flavored Markdown inline syntax (autolinks and strikethrough in
    /// addition to CommonMark; table/task ownership remains in the spine).
    Gfm,
}

/// A block-spine classification. Non-inline block kinds are representable so
/// accidental delegation fails closed rather than silently parsing text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineInputKind {
    Paragraph,
    Heading { level: u8, setext: bool },
    ListItemParagraph,
    TableCell,
    RawBlock,
    ReferenceDefinition,
    Document,
}

impl InlineInputKind {
    fn node_value(self) -> Option<NodeValue> {
        match self {
            Self::Paragraph | Self::ListItemParagraph => Some(NodeValue::Paragraph),
            Self::Heading { level, setext } if (1..=6).contains(&level) => {
                Some(NodeValue::Heading(NodeHeading {
                    level,
                    setext,
                    closed: false,
                }))
            }
            Self::TableCell => Some(NodeValue::TableCell),
            Self::Heading { .. } | Self::RawBlock | Self::ReferenceDefinition | Self::Document => {
                None
            }
        }
    }
}

/// Immutable, document-owned first-definition-wins reference index. The
/// parser calls it only for labels actually used by this leaf.
pub trait InlineReferenceSnapshot: Debug + RefUnwindSafe + Send + Sync {
    fn identity(&self) -> u64;
    fn generation(&self) -> u64;
    fn resolve(&self, normalized: &str, original: &str) -> InlineReferenceTarget;
}

/// One symbol-table lookup. `presence_generation` changes only when a label
/// crosses undefined/defined; URL/title-only winner changes update the symbol
/// table without invalidating leaf structure. Values deliberately do not cross
/// this parser boundary, so a tiny leaf cannot clone a huge definition value.
#[derive(Clone, Debug)]
pub struct InlineReferenceTarget {
    pub symbol_id: u64,
    pub presence_generation: u64,
    pub defined: bool,
}

/// Explicit empty resolver for documents with no reference definitions. A
/// resolver is mandatory so misses remain observable dependencies.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyReferenceSnapshot;

impl InlineReferenceSnapshot for EmptyReferenceSnapshot {
    fn identity(&self) -> u64 {
        0
    }

    fn generation(&self) -> u64 {
        0
    }

    fn resolve(&self, normalized: &str, _original: &str) -> InlineReferenceTarget {
        InlineReferenceTarget {
            // Research convenience only. Production snapshots allocate
            // collision-free IDs from the document symbol table; dependencies
            // retain the normalized label as a collision guard.
            symbol_id: stable_empty_symbol_id(normalized),
            presence_generation: 0,
            defined: false,
        }
    }
}

pub static EMPTY_REFERENCE_SNAPSHOT: EmptyReferenceSnapshot = EmptyReferenceSnapshot;

fn stable_empty_symbol_id(label: &str) -> u64 {
    label
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

struct ReferenceResolverAdapter<'a>(&'a dyn InlineReferenceSnapshot);

impl InlineReferenceResolver for ReferenceResolverAdapter<'_> {
    fn resolve_inline_reference(
        &self,
        normalized: &str,
        original: &str,
    ) -> ParserInlineReferenceResolution {
        let target = self.0.resolve(normalized, original);
        ParserInlineReferenceResolution {
            symbol_id: target.symbol_id,
            presence_generation: target.presence_generation,
            reference: target.defined.then(|| ResolvedReference {
                url: String::new(),
                title: String::new(),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReferenceDependency {
    pub normalized_label: String,
    pub symbol_id: u64,
    pub presence_generation: u64,
    pub resolved: bool,
}

/// Complete inline request. `logical` is the block-spine-owned inline content:
/// it includes every interior line ending and may include the terminal line
/// ending/trailing spaces accumulated by the block parser. The service applies
/// Comrak's `rtrim`; a terminal suffix emits no break/marker fact, while an
/// interior CRLF remains two logical bytes. `expected_revision` is the worker's
/// current source revision; a stale leaf is rejected before allocating an
/// arena.
#[derive(Clone, Copy, Debug)]
pub struct InlineFragmentRequest<'a> {
    pub logical: &'a str,
    pub leaf_id: u64,
    pub kind: InlineInputKind,
    pub profile: InlineProfile,
    pub reference_snapshot: &'a dyn InlineReferenceSnapshot,
    pub revision: u64,
    pub expected_revision: u64,
}

/// A fixed-width, leaf-logical inline fact. The Rust representation is 20
/// bytes and can be copied directly into a packed protocol page.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineFact {
    pub kind: u8,
    pub flags: u8,
    pub depth: u16,
    pub logical_start: u32,
    pub logical_len: u32,
    pub payload_start: u32,
    pub payload_len: u32,
}

const _: () = assert!(mem::size_of::<InlineFact>() == 20);

/// Node tags used in [`InlineFact::kind`].
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineFactKind {
    Text = 1,
    SoftBreak = 2,
    LineBreak = 3,
    Code = 4,
    HtmlInline = 5,
    Emphasis = 6,
    Strong = 7,
    Strikethrough = 8,
    Link = 9,
    Image = 10,
    Escaped = 11,
    /// A parser-owned task-list marker. Its logical range is the exact symbol
    /// byte (` `, `x`, or `X`); the complete removed `[ ] `/`[x] ` prefix is
    /// emitted independently as a hidden-marker projection fact.
    TaskListMarker = 12,
}

/// Set on Link/Image facts whose payload is a little-endian `u64` document
/// symbol ID rather than a cloned URL/title pair.
pub const INLINE_FACT_FLAG_REFERENCE_SYMBOL: u8 = 1;

/// Set on Text facts reconstructed from their logical source range plus
/// projection facts. Their payload slice is intentionally empty.
pub const INLINE_FACT_FLAG_SOURCE_BACKED: u8 = 2;

/// Set on a [`InlineFactKind::TaskListMarker`] fact when Comrak classified the
/// strict task marker as checked (`x` or `X`).
pub const INLINE_FACT_FLAG_TASK_CHECKED: u8 = 4;

/// Tags for parser-owned source projection facts. These share the packed fact
/// wire shape but are stored separately from semantic AST facts.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineProjectionFactKind {
    HiddenMarker = 32,
    Replacement = 33,
}

/// Compact result. Payloads are concatenated as length-delimited fields in
/// `payload`; facts reference their own slice. URLs/titles are encoded as
/// `[url_len:u32-le][url][title]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineFragment {
    pub leaf_id: u64,
    pub revision: u64,
    /// Diagnostic snapshot metadata only. It is not a leaf validity key.
    pub reference_snapshot_identity: u64,
    /// Diagnostic root generation only. Leaf invalidation compares the stable
    /// per-symbol dependency and presence state instead.
    pub reference_snapshot_generation: u64,
    pub reference_dependencies: Vec<ReferenceDependency>,
    pub facts: Vec<InlineFact>,
    pub projection_facts: Vec<InlineFact>,
    pub payload: Vec<u8>,
}

impl InlineFragment {
    pub fn output_bytes(&self) -> usize {
        (self.facts.len() + self.projection_facts.len()) * mem::size_of::<InlineFact>()
            + self.payload.len()
            + self
                .reference_dependencies
                .iter()
                .map(|dependency| 21 + dependency.normalized_label.len())
                .sum::<usize>()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InlineFragmentError {
    EmptyCapInvariant,
    OverCap {
        bytes: usize,
        cap: usize,
    },
    StaleRevision {
        leaf_id: u64,
        actual: u64,
        expected: u64,
    },
    UnsupportedInputKind(InlineInputKind),
    LogicalRangeInvalid,
    UnsupportedInlineNode(&'static str),
    TooManyFacts,
    PayloadTooLarge,
    TooManyReferenceDependencies,
    OutputTooLarge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TaskListMarker {
    marker: Range<usize>,
    symbol: Range<usize>,
    checked: bool,
}

/// Parse one certified inline-bearing leaf without invoking Comrak's block
/// parser. Parser work has a hard input cap and the returned protocol has
/// independent fact, payload, dependency, and total-output ceilings.
pub fn parse_inline_fragment(
    request: InlineFragmentRequest<'_>,
) -> Result<InlineFragment, InlineFragmentError> {
    if MAX_INLINE_FRAGMENT_BYTES == 0 {
        return Err(InlineFragmentError::EmptyCapInvariant);
    }
    if request.logical.len() > MAX_INLINE_FRAGMENT_BYTES {
        return Err(InlineFragmentError::OverCap {
            bytes: request.logical.len(),
            cap: MAX_INLINE_FRAGMENT_BYTES,
        });
    }
    if request.revision != request.expected_revision {
        return Err(InlineFragmentError::StaleRevision {
            leaf_id: request.leaf_id,
            actual: request.revision,
            expected: request.expected_revision,
        });
    }
    let value = request
        .kind
        .node_value()
        .ok_or(InlineFragmentError::UnsupportedInputKind(request.kind))?;

    let options = options_for(request.profile);
    let arena = Arena::new();
    let sourcepos = fragment_sourcepos(request.logical);
    let mut ast = Ast::new(value, (1, 1).into());
    ast.sourcepos = sourcepos;
    ast.line_offsets = vec![0; logical_line_starts(request.logical).len()];
    let parent = arena.alloc(ast.into());

    let mut refmap = RefMap::new();

    let mut content = request.logical.to_owned();
    strings::rtrim(&mut content);
    let delimiter_arena = typed_arena::Arena::new();
    let mut footnote_defs = FootnoteDefs::new();
    let reference_resolver = ReferenceResolverAdapter(request.reference_snapshot);
    let mut annotations = {
        let mut parent_ast = parent.data_mut();
        let mut subject = Subject::new(
            &arena,
            &options,
            content,
            1,
            &mut refmap,
            &mut footnote_defs,
            &delimiter_arena,
            0,
        );
        subject.enable_annotations();
        subject.set_inline_reference_resolver(&reference_resolver);
        while subject.parse_inline(parent, &mut parent_ast) {}
        subject.process_emphasis(0);
        subject.clear_brackets();
        subject.take_annotations()
    };

    // The block spine certifies that this leaf is the first paragraph under an
    // Item under a List. Preserve Comrak's remaining precedence by scanning
    // only after inline parsing and only when its first child is Text: a
    // resolved `[x]` Link or an escaped opener must not become a task.
    let task_marker = if request.profile == InlineProfile::Gfm
        && request.kind == InlineInputKind::ListItemParagraph
    {
        process_task_list_marker(parent, request.logical)?
    } else {
        None
    };
    if let Some(marker) = &task_marker {
        annotations.push(InlineAnnotation {
            kind: InlineAnnotationKind::Marker,
            start: marker.marker.start,
            end: marker.marker.end,
        });
    }
    if request.profile == InlineProfile::Gfm {
        process_email_autolinks(parent, &arena, &options);
    }

    let mut reference_dependencies = annotations
        .iter()
        .filter_map(|annotation| match &annotation.kind {
            InlineAnnotationKind::ReferenceQuery {
                normalized_label,
                symbol_id,
                presence_generation,
                resolved,
                ..
            } => Some(ReferenceDependency {
                symbol_id: *symbol_id,
                presence_generation: *presence_generation,
                normalized_label: normalized_label.clone(),
                resolved: *resolved,
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    reference_dependencies.sort_unstable();
    reference_dependencies.dedup();
    if reference_dependencies.len() > MAX_REFERENCE_DEPENDENCIES {
        return Err(InlineFragmentError::TooManyReferenceDependencies);
    }
    compact(
        parent,
        request,
        &annotations,
        reference_dependencies,
        task_marker,
    )
}

fn process_task_list_marker(
    paragraph: crate::nodes::Node<'_>,
    logical: &str,
) -> Result<Option<TaskListMarker>, InlineFragmentError> {
    let Some(node) = paragraph.first_child() else {
        return Ok(None);
    };

    let line_starts = logical_line_starts(logical);
    let (marker, symbol, checked, emptied) = {
        let mut ast = node.data_mut();
        let sourcepos = ast.sourcepos;
        let NodeValue::Text(ref mut text) = ast.value else {
            return Ok(None);
        };
        let mut adjusted = sourcepos;
        let source_runs = coalesce_adjacent_text(node, text, &mut adjusted);
        let Some((end, symbol_decoded, checked)) =
            crate::scanners::tasklist(text).and_then(|(end, matched, symbol)| {
                let mut chars = matched.chars();
                let value = chars.next()?;
                if chars.next().is_some() || !matches!(value, ' ' | 'x' | 'X') {
                    return None;
                }
                Some((end, symbol, value != ' '))
            })
        else {
            // Coalescing is semantically neutral even when the task scanner
            // does not match. Persist the expanded range before this early
            // return so unmatched adjacent Text (for example an unmatched
            // code delimiter) retains every source-backed byte.
            ast.sourcepos = adjusted;
            return Ok(None);
        };

        let marker = decoded_source_range(logical, &line_starts, &source_runs, 0..end)?;
        let symbol = decoded_source_range(logical, &line_starts, &source_runs, symbol_decoded)?;
        let mut consumed = Spx(source_runs);
        adjusted.start.column = consumed.consume(end) + 1;
        strings::remove_from_start(text.to_mut(), end);
        let emptied = text.is_empty();
        ast.sourcepos = adjusted;
        (marker, symbol, checked, emptied)
    };

    if emptied {
        node.detach();
    }

    if marker.start >= marker.end || symbol.start < marker.start || symbol.end > marker.end {
        return Err(InlineFragmentError::LogicalRangeInvalid);
    }

    Ok(Some(TaskListMarker {
        marker,
        symbol,
        checked,
    }))
}

fn decoded_source_range(
    logical: &str,
    line_starts: &[usize],
    source_runs: &VecDeque<(Sourcepos, usize)>,
    decoded: Range<usize>,
) -> Result<Range<usize>, InlineFragmentError> {
    let Some((first, _)) = source_runs.front() else {
        return Err(InlineFragmentError::LogicalRangeInvalid);
    };
    if decoded.start >= decoded.end {
        return Err(InlineFragmentError::LogicalRangeInvalid);
    }
    let mut start = Spx(source_runs.clone());
    let start_column = start.consume(decoded.start) + 1;
    let mut end = Spx(source_runs.clone());
    let end_column = end.consume(decoded.end);
    sourcepos_to_range(
        (first.start.line, start_column, first.start.line, end_column).into(),
        logical,
        line_starts,
    )
}

fn coalesce_adjacent_text(
    node: crate::nodes::Node<'_>,
    text: &mut Cow<'static, str>,
    sourcepos: &mut Sourcepos,
) -> VecDeque<(Sourcepos, usize)> {
    let mut source_runs = VecDeque::from([(*sourcepos, text.len())]);
    while let Some(adjacent) = node.next_sibling() {
        let adjacent_data = adjacent.data();
        let NodeValue::Text(adjacent_text) = &adjacent_data.value else {
            break;
        };
        text.to_mut().push_str(adjacent_text);
        source_runs.push_back((adjacent_data.sourcepos, adjacent_text.len()));
        sourcepos.end = adjacent_data.sourcepos.end;
        drop(adjacent_data);
        adjacent.detach();
    }
    source_runs
}

fn options_for(profile: InlineProfile) -> Options<'static> {
    let mut options = Options::default();
    if profile == InlineProfile::Gfm {
        options.extension.strikethrough = true;
        options.extension.tagfilter = true;
        options.extension.table = true;
        options.extension.autolink = true;
        options.extension.tasklist = true;
    }
    options
}

fn logical_line_starts(logical: &str) -> Vec<usize> {
    let bytes = logical.as_bytes();
    let mut starts = vec![0];
    let mut offset = 0;
    while offset < bytes.len() {
        match bytes[offset] {
            b'\r' if bytes.get(offset + 1) == Some(&b'\n') => {
                offset += 2;
                starts.push(offset);
            }
            b'\r' | b'\n' => {
                offset += 1;
                starts.push(offset);
            }
            _ => offset += 1,
        }
    }
    starts
}

fn fragment_sourcepos(logical: &str) -> Sourcepos {
    let starts = logical_line_starts(logical);
    let line = starts.len();
    let column = logical.len().saturating_sub(starts[line - 1]) + 1;
    (1, 1, line, column).into()
}

fn compact(
    parent: crate::nodes::Node<'_>,
    request: InlineFragmentRequest<'_>,
    annotations: &[InlineAnnotation],
    reference_dependencies: Vec<ReferenceDependency>,
    task_marker: Option<TaskListMarker>,
) -> Result<InlineFragment, InlineFragmentError> {
    let line_starts = logical_line_starts(request.logical);
    let mut facts = Vec::new();
    let mut projection_facts = Vec::new();
    let mut payload = Vec::new();
    if let Some(marker) = task_marker {
        facts.push(InlineFact {
            kind: InlineFactKind::TaskListMarker as u8,
            flags: if marker.checked {
                INLINE_FACT_FLAG_TASK_CHECKED
            } else {
                0
            },
            depth: 0,
            logical_start: u32::try_from(marker.symbol.start)
                .map_err(|_| InlineFragmentError::LogicalRangeInvalid)?,
            logical_len: u32::try_from(marker.symbol.len())
                .map_err(|_| InlineFragmentError::LogicalRangeInvalid)?,
            payload_start: 0,
            payload_len: 0,
        });
    }
    let link_overrides: Vec<_> = annotations
        .iter()
        .filter_map(|annotation| match annotation.kind {
            InlineAnnotationKind::LinkSpan { image } => {
                Some((annotation.start, annotation.end, image))
            }
            _ => None,
        })
        .collect();
    let reference_links: Vec<_> = annotations
        .iter()
        .filter_map(|annotation| match annotation.kind {
            InlineAnnotationKind::ReferenceQuery {
                symbol_id,
                resolved: true,
                image,
                ..
            } => Some((annotation.start, annotation.end, image, symbol_id)),
            _ => None,
        })
        .collect();

    for node in parent.descendants().skip(1) {
        if facts.len() >= MAX_INLINE_FACTS {
            return Err(InlineFragmentError::TooManyFacts);
        }
        let ast = node.data();
        let mut logical = sourcepos_to_range(ast.sourcepos, request.logical, &line_starts)?;
        let link_image = match ast.value {
            NodeValue::Link(_) => Some(false),
            NodeValue::Image(_) => Some(true),
            _ => None,
        };
        if let Some(image) = link_image {
            if let Some((_, end, _)) = link_overrides.iter().find(|(start, _, candidate_image)| {
                *start == logical.start && *candidate_image == image
            }) {
                logical.end = *end;
            }
        }
        let reference_symbol = link_image.and_then(|image| {
            reference_links
                .iter()
                .find(|(start, _, candidate_image, _)| {
                    *start == logical.start && *candidate_image == image
                })
                .map(|(_, _, _, symbol_id)| *symbol_id)
        });
        let (kind, flags, encoded_payload) = encode_value(&ast.value, reference_symbol)?;
        let payload_start =
            u32::try_from(payload.len()).map_err(|_| InlineFragmentError::PayloadTooLarge)?;
        let payload_len = u32::try_from(encoded_payload.len())
            .map_err(|_| InlineFragmentError::PayloadTooLarge)?;
        extend_payload(&mut payload, &encoded_payload)?;
        let depth = u16::try_from(node.ancestors().skip(1).count())
            .map_err(|_| InlineFragmentError::TooManyFacts)?;
        facts.push(InlineFact {
            kind: kind as u8,
            flags,
            depth,
            logical_start: u32::try_from(logical.start)
                .map_err(|_| InlineFragmentError::LogicalRangeInvalid)?,
            logical_len: u32::try_from(logical.len())
                .map_err(|_| InlineFragmentError::LogicalRangeInvalid)?,
            payload_start,
            payload_len,
        });
    }
    compact_annotations(
        annotations,
        request,
        facts.len(),
        &mut projection_facts,
        &mut payload,
    )?;
    let fragment = InlineFragment {
        leaf_id: request.leaf_id,
        revision: request.revision,
        reference_snapshot_identity: request.reference_snapshot.identity(),
        reference_snapshot_generation: request.reference_snapshot.generation(),
        reference_dependencies,
        facts,
        projection_facts,
        payload,
    };
    if fragment.output_bytes() > MAX_INLINE_OUTPUT_BYTES {
        return Err(InlineFragmentError::OutputTooLarge);
    }
    Ok(fragment)
}

fn compact_annotations(
    annotations: &[InlineAnnotation],
    request: InlineFragmentRequest<'_>,
    semantic_fact_count: usize,
    facts: &mut Vec<InlineFact>,
    payload: &mut Vec<u8>,
) -> Result<(), InlineFragmentError> {
    let mut annotations = annotations.to_vec();
    annotations.sort_by(|left, right| {
        (left.start, left.end, annotation_order(&left.kind)).cmp(&(
            right.start,
            right.end,
            annotation_order(&right.kind),
        ))
    });
    annotations.dedup();

    for annotation in annotations {
        if annotation.start >= annotation.end || annotation.end > request.logical.len() {
            return Err(InlineFragmentError::LogicalRangeInvalid);
        }
        let (kind, replacement) = match annotation.kind {
            InlineAnnotationKind::Marker => (InlineProjectionFactKind::HiddenMarker, None),
            InlineAnnotationKind::Replacement(replacement) => {
                if request.logical.as_bytes()[annotation.start..annotation.end]
                    == *replacement.as_bytes()
                {
                    continue;
                }
                (InlineProjectionFactKind::Replacement, Some(replacement))
            }
            InlineAnnotationKind::LinkSpan { .. } | InlineAnnotationKind::ReferenceQuery { .. } => {
                continue;
            }
        };
        if semantic_fact_count + facts.len() >= MAX_INLINE_FACTS {
            return Err(InlineFragmentError::TooManyFacts);
        }
        let payload_start =
            u32::try_from(payload.len()).map_err(|_| InlineFragmentError::PayloadTooLarge)?;
        let payload_len = replacement.as_ref().map_or(Ok(0), |value| {
            u32::try_from(value.len()).map_err(|_| InlineFragmentError::PayloadTooLarge)
        })?;
        if let Some(replacement) = replacement {
            extend_payload(payload, replacement.as_bytes())?;
        }
        facts.push(InlineFact {
            kind: kind as u8,
            flags: 0,
            depth: 0,
            logical_start: u32::try_from(annotation.start)
                .map_err(|_| InlineFragmentError::LogicalRangeInvalid)?,
            logical_len: u32::try_from(annotation.end - annotation.start)
                .map_err(|_| InlineFragmentError::LogicalRangeInvalid)?,
            payload_start,
            payload_len,
        });
    }
    Ok(())
}

fn annotation_order(kind: &InlineAnnotationKind) -> u8 {
    match kind {
        InlineAnnotationKind::Marker => 0,
        InlineAnnotationKind::Replacement(_) => 1,
        InlineAnnotationKind::LinkSpan { .. } => 2,
        InlineAnnotationKind::ReferenceQuery { .. } => 3,
    }
}

fn extend_payload(payload: &mut Vec<u8>, addition: &[u8]) -> Result<(), InlineFragmentError> {
    let next_len = payload
        .len()
        .checked_add(addition.len())
        .ok_or(InlineFragmentError::PayloadTooLarge)?;
    if next_len > MAX_INLINE_PAYLOAD_BYTES {
        return Err(InlineFragmentError::PayloadTooLarge);
    }
    payload.extend_from_slice(addition);
    Ok(())
}

fn process_email_autolinks<'a>(
    parent: crate::nodes::Node<'a>,
    arena: &'a Arena<'a>,
    options: &Options<'_>,
) {
    // Mirror `Parser::postprocess_text_nodes`: inline parsing may leave one
    // semantic text run in several adjacent Text nodes (notably around an
    // underscore delimiter). GFM email recognition is defined over the
    // coalesced run, not each allocation separately. Bracket descendants are
    // traversed for text coalescing but never receive nested autolinks.
    let mut stack = vec![(parent, false)];
    let mut children = Vec::new();
    while let Some((container, in_bracket_context)) = stack.pop() {
        let mut next = container.first_child();
        while let Some(node) = next {
            let mut child_in_bracket_context = in_bracket_context;
            let mut emptied = false;
            {
                let mut ast = node.data_mut();
                let sourcepos = ast.sourcepos;
                match ast.value {
                    NodeValue::Text(ref mut text) => {
                        let mut adjusted = sourcepos;
                        let source_runs = coalesce_adjacent_text(node, text, &mut adjusted);
                        if !in_bracket_context {
                            let mut spx = Spx(source_runs);
                            autolink::process_email_autolinks(
                                arena,
                                node,
                                text,
                                options.parse.relaxed_autolinks,
                                &mut adjusted,
                                &mut spx,
                            );
                        }
                        emptied = text.is_empty();
                        ast.sourcepos = adjusted;
                    }
                    NodeValue::Link(_) | NodeValue::Image(_) | NodeValue::WikiLink(_) => {
                        child_in_bracket_context = true;
                    }
                    _ => {}
                }
            }
            if !emptied {
                children.push((node, child_in_bracket_context));
            }
            next = node.next_sibling();
            if emptied {
                node.detach();
            }
        }
        stack.extend(children.drain(..).rev());
    }
}

fn sourcepos_to_range(
    sourcepos: Sourcepos,
    logical: &str,
    line_starts: &[usize],
) -> Result<Range<usize>, InlineFragmentError> {
    let start_line = sourcepos
        .start
        .line
        .checked_sub(1)
        .ok_or(InlineFragmentError::LogicalRangeInvalid)?;
    let end_line = sourcepos
        .end
        .line
        .checked_sub(1)
        .ok_or(InlineFragmentError::LogicalRangeInvalid)?;
    let start = line_starts
        .get(start_line)
        .and_then(|line| line.checked_add(sourcepos.start.column.checked_sub(1)?))
        .ok_or(InlineFragmentError::LogicalRangeInvalid)?;
    let end = line_starts
        .get(end_line)
        .and_then(|line| line.checked_add(sourcepos.end.column))
        .ok_or(InlineFragmentError::LogicalRangeInvalid)?;
    if start > end
        || end > logical.len()
        || !logical.is_char_boundary(start)
        || !logical.is_char_boundary(end)
    {
        return Err(InlineFragmentError::LogicalRangeInvalid);
    }
    Ok(start..end)
}

fn encode_value(
    value: &NodeValue,
    reference_symbol: Option<u64>,
) -> Result<(InlineFactKind, u8, Vec<u8>), InlineFragmentError> {
    let result = match value {
        #[cfg(feature = "flark-inline-owned-text")]
        NodeValue::Text(text) => (InlineFactKind::Text, 0, text.as_bytes().to_vec()),
        #[cfg(not(feature = "flark-inline-owned-text"))]
        NodeValue::Text(_) => (
            InlineFactKind::Text,
            INLINE_FACT_FLAG_SOURCE_BACKED,
            Vec::new(),
        ),
        NodeValue::SoftBreak => (InlineFactKind::SoftBreak, 0, Vec::new()),
        NodeValue::LineBreak => (InlineFactKind::LineBreak, 0, Vec::new()),
        NodeValue::Code(code) => {
            let mut payload = u32::try_from(code.num_backticks)
                .unwrap_or(u32::MAX)
                .to_le_bytes()
                .to_vec();
            payload.extend_from_slice(code.literal.as_bytes());
            (InlineFactKind::Code, 0, payload)
        }
        NodeValue::HtmlInline(html) => (InlineFactKind::HtmlInline, 0, html.as_bytes().to_vec()),
        NodeValue::Emph => (InlineFactKind::Emphasis, 0, Vec::new()),
        NodeValue::Strong => (InlineFactKind::Strong, 0, Vec::new()),
        NodeValue::Strikethrough => (InlineFactKind::Strikethrough, 0, Vec::new()),
        NodeValue::Link(link) => encode_link_fact(InlineFactKind::Link, link, reference_symbol)?,
        NodeValue::Image(link) => encode_link_fact(InlineFactKind::Image, link, reference_symbol)?,
        NodeValue::Escaped => (InlineFactKind::Escaped, 0, Vec::new()),
        other => {
            return Err(InlineFragmentError::UnsupportedInlineNode(
                other.xml_node_name(),
            ));
        }
    };
    Ok(result)
}

fn encode_link_fact(
    kind: InlineFactKind,
    link: &crate::nodes::NodeLink,
    reference_symbol: Option<u64>,
) -> Result<(InlineFactKind, u8, Vec<u8>), InlineFragmentError> {
    Ok(match reference_symbol {
        Some(symbol_id) => (
            kind,
            INLINE_FACT_FLAG_REFERENCE_SYMBOL,
            symbol_id.to_le_bytes().to_vec(),
        ),
        None => (kind, 0, encode_link(&link.url, &link.title)?),
    })
}

fn encode_link(url: &str, title: &str) -> Result<Vec<u8>, InlineFragmentError> {
    let url_len = u32::try_from(url.len()).map_err(|_| InlineFragmentError::PayloadTooLarge)?;
    let mut payload = url_len.to_le_bytes().to_vec();
    payload.extend_from_slice(url.as_bytes());
    payload.extend_from_slice(title.as_bytes());
    Ok(payload)
}
