//! Scalar storage observation at one physical source coordinate.
//!
//! A source coordinate does not uniquely determine a green event cut: any
//! number of zero-metric Enter/Exit events may occur between adjacent coverage
//! runs. This resolver therefore requires an explicit adjacent-side bias and
//! returns only an observation of that exact Coverage side. It is not a parser
//! checkpoint, prefix/suffix cut, or adoption capability.

use crate::{ArenaId, CoverageId, PageArena};

use super::{
    DecodedGreenEventKind, Decoder, FactField, GreenCoverageCapability, GreenSummary,
    SequenceNodeKind, SerializedGreenDocument, SerializedGreenError,
    SerializedGreenManifestDescriptor, SerializedGreenManifestId, SerializedGreenSpec,
    SerializedMetric, decode_document, sequence_node, visit_decoded_leaf_events,
};

/// Which adjacent physical Coverage run a storage query must corroborate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Production cannot mint checkpoint-index permits in this feasibility slice.
pub(crate) enum SerializedGreenCoverageSideBias {
    BeforeFollowing,
    AfterPreceding,
}

/// Exact page-local Coverage side observed at one source coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SerializedGreenAdjacentCoverageSide {
    EmptyDocument { manifest: SerializedGreenManifestId },
    BeforeFollowing(GreenCoverageCapability),
    AfterPreceding(GreenCoverageCapability),
}

/// Storage-only observation returned by one decoded manifest descent.
///
/// The type is O(1), owns no arena page, and intentionally contains no parser
/// state or general green sequence-cut authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SerializedGreenCoverageSideObservation {
    pub(crate) manifest: SerializedGreenManifestDescriptor,
    pub(crate) source_cut: SerializedMetric,
    pub(crate) adjacent: SerializedGreenAdjacentCoverageSide,
}

/// Exact bounded-work accounting for one adjacent-Coverage query.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SerializedGreenCoverageSideReceipt {
    pub(crate) root_descents: usize,
    pub(crate) sequence_nodes_read: usize,
    pub(crate) maximum_route_depth: usize,
    pub(crate) leaf_pages_scanned: usize,
    pub(crate) events_inspected: usize,
    pub(crate) borrowed_leaf_payload_bytes: usize,
    /// The scalar visitor never constructs a decoded-event Vec.
    pub(crate) decoded_event_capacity: usize,
    /// Maximum logical heap bytes owned by any one transient decoded event.
    pub(crate) maximum_transient_event_heap_bytes: usize,
    /// Inline scalar decoder/visitor state; excludes the field above.
    pub(crate) maximum_scanner_scratch_bytes: usize,
    /// Returned observation and receipt retain no heap allocation.
    pub(crate) retained_heap_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SerializedGreenCoverageSideOutcome {
    Found {
        observation: SerializedGreenCoverageSideObservation,
        receipt: SerializedGreenCoverageSideReceipt,
    },
    NoAdjacentCoverage(SerializedGreenCoverageSideReceipt),
    NotCoverageBoundary(SerializedGreenCoverageSideReceipt),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SelectedCoverage {
    byte_offset: u16,
    id: CoverageId,
    relative_start: SerializedMetric,
    relative_end: SerializedMetric,
}

impl SerializedGreenDocument {
    /// Observes one exact adjacent Coverage side without reconstructing a
    /// structural open path or retaining decoded-event storage.
    ///
    /// The raw coordinate and bias are Stage-0 mechanism inputs. Production
    /// checkpoint code must derive both from a stored checkpoint-index entry.
    pub(crate) fn resolve_storage_source_coverage_side(
        &self,
        arena: &PageArena,
        source_byte_cut: u64,
        bias: SerializedGreenCoverageSideBias,
    ) -> Result<SerializedGreenCoverageSideOutcome, SerializedGreenError> {
        let manifest_id = self.local_manifest_id(arena)?;
        let manifest_capability = self.manifest_id();
        let (manifest, root) = decode_document(arena, manifest_id)?;
        if source_byte_cut > manifest.source_bytes {
            return Err(SerializedGreenError::SourceOutOfBounds);
        }
        let mut receipt = SerializedGreenCoverageSideReceipt {
            root_descents: 1,
            maximum_scanner_scratch_bytes: std::mem::size_of::<Decoder<'_>>()
                + std::mem::size_of::<DecodedGreenEventKind>()
                + std::mem::size_of::<SelectedCoverage>()
                + std::mem::size_of::<GreenSummary>() * 2,
            ..SerializedGreenCoverageSideReceipt::default()
        };
        let binding = SerializedGreenManifestDescriptor::new(manifest_capability, &manifest);
        if manifest.source_bytes == 0 {
            debug_assert_eq!(source_byte_cut, 0);
            return Ok(SerializedGreenCoverageSideOutcome::Found {
                observation: SerializedGreenCoverageSideObservation {
                    manifest: binding,
                    source_cut: SerializedMetric::default(),
                    adjacent: SerializedGreenAdjacentCoverageSide::EmptyDocument {
                        manifest: manifest_capability,
                    },
                },
                receipt,
            });
        }

        let local_probe = match bias {
            SerializedGreenCoverageSideBias::BeforeFollowing => {
                if source_byte_cut == manifest.source_bytes {
                    return Ok(SerializedGreenCoverageSideOutcome::NoAdjacentCoverage(
                        receipt,
                    ));
                }
                source_byte_cut
            }
            SerializedGreenCoverageSideBias::AfterPreceding => {
                let Some(probe) = source_byte_cut.checked_sub(1) else {
                    return Ok(SerializedGreenCoverageSideOutcome::NoAdjacentCoverage(
                        receipt,
                    ));
                };
                probe
            }
        };
        let (leaf, leaf_index, base, probe_within_leaf) =
            descend_source_leaf(arena, root, local_probe, &mut receipt)?;
        receipt.leaf_pages_scanned = 1;
        receipt.borrowed_leaf_payload_bytes = arena.payload(leaf)?.len();
        let selected = scan_leaf_for_probe(arena, leaf, probe_within_leaf, &mut receipt)?;
        let absolute_start = base.checked_add(selected.relative_start)?;
        let absolute_end = base.checked_add(selected.relative_end)?;
        let capability = GreenCoverageCapability {
            manifest: manifest_capability,
            leaf,
            base_leaf_index: leaf_index,
            byte_offset: selected.byte_offset,
            coverage: selected.id,
        };
        let (source_cut, adjacent) = match bias {
            SerializedGreenCoverageSideBias::BeforeFollowing
                if source_byte_cut == absolute_start.bytes =>
            {
                (
                    absolute_start,
                    SerializedGreenAdjacentCoverageSide::BeforeFollowing(capability),
                )
            }
            SerializedGreenCoverageSideBias::AfterPreceding
                if source_byte_cut == absolute_end.bytes =>
            {
                (
                    absolute_end,
                    SerializedGreenAdjacentCoverageSide::AfterPreceding(capability),
                )
            }
            SerializedGreenCoverageSideBias::BeforeFollowing
            | SerializedGreenCoverageSideBias::AfterPreceding => {
                return Ok(SerializedGreenCoverageSideOutcome::NotCoverageBoundary(
                    receipt,
                ));
            }
        };
        Ok(SerializedGreenCoverageSideOutcome::Found {
            observation: SerializedGreenCoverageSideObservation {
                manifest: binding,
                source_cut,
                adjacent,
            },
            receipt,
        })
    }
}

fn descend_source_leaf(
    arena: &PageArena,
    root: ArenaId,
    mut probe: u64,
    receipt: &mut SerializedGreenCoverageSideReceipt,
) -> Result<(ArenaId, u64, SerializedMetric, u64), SerializedGreenError> {
    let mut node = root;
    let mut base = SerializedMetric::default();
    let mut leaf_index = 0_u64;
    let mut route_depth = 0_usize;
    loop {
        receipt.sequence_nodes_read =
            receipt
                .sequence_nodes_read
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "coverage-side sequence-node receipt",
                ))?;
        match sequence_node::<SerializedGreenSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => return Ok((node, leaf_index, base, probe)),
            SequenceNodeKind::Branch { left, right } => {
                receipt.sequence_nodes_read = receipt.sequence_nodes_read.checked_add(1).ok_or(
                    SerializedGreenError::Overflow("coverage-side sequence-node receipt"),
                )?;
                let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
                if probe < left_summary.metric.bytes {
                    node = left;
                } else {
                    probe -= left_summary.metric.bytes;
                    base = base.checked_add(left_summary.metric)?;
                    leaf_index = leaf_index
                        .checked_add(left_summary.leaves)
                        .ok_or(SerializedGreenError::Overflow("leaf index"))?;
                    node = right;
                }
                route_depth = route_depth
                    .checked_add(1)
                    .ok_or(SerializedGreenError::Overflow("coverage-side route depth"))?;
                receipt.maximum_route_depth = receipt.maximum_route_depth.max(route_depth);
            }
        }
    }
}

fn scan_leaf_for_probe(
    arena: &PageArena,
    leaf: ArenaId,
    probe: u64,
    receipt: &mut SerializedGreenCoverageSideReceipt,
) -> Result<SelectedCoverage, SerializedGreenError> {
    let mut within = SerializedMetric::default();
    let mut selected = None;
    visit_decoded_leaf_events(arena, leaf, |byte_offset, event| {
        receipt.events_inspected =
            receipt
                .events_inspected
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "coverage-side event receipt",
                ))?;
        receipt.maximum_transient_event_heap_bytes = receipt
            .maximum_transient_event_heap_bytes
            .max(transient_event_heap_bytes(&event)?);
        if let DecodedGreenEventKind::Coverage(run) = event {
            let end = within.checked_add(run.metric)?;
            if selected.is_none() && probe < end.bytes {
                selected = Some(SelectedCoverage {
                    byte_offset,
                    id: run.id,
                    relative_start: within,
                    relative_end: end,
                });
            }
            within = end;
        }
        Ok(())
    })?;
    selected.ok_or(SerializedGreenError::Corrupt(
        "coverage-side descent leaf has no matching coverage",
    ))
}

fn transient_event_heap_bytes(
    event: &DecodedGreenEventKind,
) -> Result<usize, SerializedGreenError> {
    let DecodedGreenEventKind::Enter { facts, .. } = event else {
        return Ok(0);
    };
    facts.fields.iter().try_fold(
        facts
            .fields
            .capacity()
            .checked_mul(std::mem::size_of::<FactField>())
            .ok_or(SerializedGreenError::Overflow(
                "coverage-side transient fact storage",
            ))?,
        |bytes, field| {
            bytes
                .checked_add(field.value.capacity())
                .ok_or(SerializedGreenError::Overflow(
                    "coverage-side transient fact storage",
                ))
        },
    )
}
