//! Canonical candidate crop/build planning and credited publication streaming.
//!
//! Admission, scheduling, delivery installation, cancellation, and cleanup remain
//! owned by `CandidateEndpoint`; this module owns exact crop/build preparation,
//! offer construction, and the prepared candidate packet stream.

use super::*;

/// Maximum base or target semantic entries inspected atomically while
/// deciding whether an exact-clean fallback can reuse packed block pages.
///
/// Larger affected regions remain correct by taking the existing full
/// publication fallback; general restart/convergence will make discovery
/// itself incremental in a later cut.
const EXACT_CLEAN_BLOCK_SPLICE_MAX_AFFECTED_ENTRIES: u64 = 256;

pub(super) fn begin_exact_candidate_build(
    runtime: &mut DocumentRuntime,
    context: CandidateContext,
    mut base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
    mut result: M11LeadingReferencesCropResult,
) -> Result<ActiveCandidate, ExactBuildStartFailure> {
    let base_restart = match result.take_base_restart_checkpoint() {
        Ok(checkpoint) => checkpoint,
        Err(_) => {
            return Err(ExactBuildStartFailure {
                error: CandidateEndpointError::InvalidState,
                base,
            });
        }
    };
    base.restart = Some(CandidateRestartAuthority::Leading(base_restart));
    let next_restart = match result.take_next_restart_checkpoint() {
        Ok(checkpoint) => checkpoint,
        Err(_) => {
            return Err(ExactBuildStartFailure {
                error: CandidateEndpointError::InvalidState,
                base,
            });
        }
    };
    let input = match result.into_exact_segmented_candidate_input() {
        Ok(input) => input,
        Err(error) => {
            return Err(ExactBuildStartFailure {
                error: error.into(),
                base,
            });
        }
    };
    let next_restart = CandidateRestartAuthority::Leading(next_restart);
    begin_exact_candidate_build_from_terminal(runtime, context, base, witness, input, next_restart)
}
pub(super) fn select_ordinary_crop_route(
    checkpoints: &M11OrdinaryParagraphRestartCheckpoints,
    base_byte_range: std::ops::Range<usize>,
) -> Result<Option<OrdinaryCropRoute>, CandidateEndpointError> {
    if base_byte_range.start == 0 && base_byte_range.end == checkpoints.source().byte_len() {
        return Ok(None);
    }
    match checkpoints.select_crop(base_byte_range.clone()) {
        Ok(selection) => Ok(Some(OrdinaryCropRoute::Interior(selection))),
        Err(M11OrdinaryParagraphCropPlanError::NoRestartCheckpoint) => {
            // The exact edit can sit inside the first Paragraph even though
            // the only valid parser crop begins at BOF. Extend only the
            // parser-selected boundary lane; the SourceFacts page range and
            // exact edit envelope remain independently authoritative.
            match checkpoints.select_bof_crop(0..base_byte_range.end) {
                Ok(selection) => Ok(Some(OrdinaryCropRoute::FromBof(selection))),
                Err(
                    M11OrdinaryParagraphBoundaryCropPlanError::SegmentedTopLevelIneligible
                    | M11OrdinaryParagraphBoundaryCropPlanError::FrozenReferencesIneligible
                    | M11OrdinaryParagraphBoundaryCropPlanError::NoConvergenceCheckpoint
                    | M11OrdinaryParagraphBoundaryCropPlanError::WholeSourceIneligible,
                ) => Ok(None),
                Err(M11OrdinaryParagraphBoundaryCropPlanError::InvalidChangedRange) => {
                    Err(CandidateEndpointError::InvalidAuthority)
                }
                Err(
                    M11OrdinaryParagraphBoundaryCropPlanError::InvalidCheckpoint
                    | M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch
                    | M11OrdinaryParagraphBoundaryCropPlanError::NotBofBoundary
                    | M11OrdinaryParagraphBoundaryCropPlanError::NotEofBoundary
                    | M11OrdinaryParagraphBoundaryCropPlanError::NoRestartCheckpoint,
                ) => Err(CandidateEndpointError::InvalidState),
            }
        }
        Err(M11OrdinaryParagraphCropPlanError::NoConvergenceCheckpoint) => {
            // Symmetrically, an edit inside the final Paragraph requires a
            // restart-to-EOF crop even when the exact edit does not touch the
            // final source byte.
            match checkpoints
                .select_eof_crop(base_byte_range.start..checkpoints.source().byte_len())
            {
                Ok(selection) => Ok(Some(OrdinaryCropRoute::ToEof(selection))),
                Err(
                    M11OrdinaryParagraphBoundaryCropPlanError::SegmentedTopLevelIneligible
                    | M11OrdinaryParagraphBoundaryCropPlanError::FrozenReferencesIneligible
                    | M11OrdinaryParagraphBoundaryCropPlanError::NoRestartCheckpoint
                    | M11OrdinaryParagraphBoundaryCropPlanError::WholeSourceIneligible,
                ) => Ok(None),
                Err(M11OrdinaryParagraphBoundaryCropPlanError::InvalidChangedRange) => {
                    Err(CandidateEndpointError::InvalidAuthority)
                }
                Err(
                    M11OrdinaryParagraphBoundaryCropPlanError::InvalidCheckpoint
                    | M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch
                    | M11OrdinaryParagraphBoundaryCropPlanError::NotBofBoundary
                    | M11OrdinaryParagraphBoundaryCropPlanError::NotEofBoundary
                    | M11OrdinaryParagraphBoundaryCropPlanError::NoConvergenceCheckpoint,
                ) => Err(CandidateEndpointError::InvalidState),
            }
        }
        Err(M11OrdinaryParagraphCropPlanError::InvalidChangedRange) => {
            Err(CandidateEndpointError::InvalidAuthority)
        }
        Err(
            M11OrdinaryParagraphCropPlanError::InvalidCheckpoint
            | M11OrdinaryParagraphCropPlanError::SelectionMismatch,
        ) => Err(CandidateEndpointError::InvalidState),
    }
}

pub(super) fn segmented_crop_exceeds_byte_cap(
    checkpoints: &M11OrdinaryParagraphRestartCheckpoints,
    selection: M11OrdinaryParagraphCropSelection,
    target_crop_start: usize,
    target_suffix_start: usize,
) -> Result<bool, CandidateEndpointError> {
    if !selection.is_segmented_top_level() {
        return Ok(false);
    }
    let convergence = checkpoints
        .checkpoints()
        .get(selection.convergence_index())
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    if convergence.source() != selection.source()
        || convergence.binding() != selection.binding()
        || convergence.paragraph_source_start_byte() != selection.convergence_suffix_start_byte()
        || convergence.paragraph_source_start_utf16() != selection.convergence_suffix_start_utf16()
    {
        return Err(CandidateEndpointError::InvalidAuthority);
    }
    let line_offset = usize::try_from(
        convergence
            .preceding_line_start_byte()
            .checked_sub(convergence.paragraph_source_start_byte())
            .ok_or(CandidateEndpointError::InvalidAuthority)?,
    )
    .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_line_end = target_suffix_start
        .checked_add(line_offset)
        .and_then(|start| {
            start.checked_add(usize::try_from(convergence.preceding_line_physical_bytes()).ok()?)
        })
        .ok_or(CandidateEndpointError::MetricOverflow)?;
    let target_crop_bytes = target_line_end
        .checked_sub(target_crop_start)
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    Ok(target_crop_bytes > M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES)
}

pub(super) fn segmented_bof_crop_exceeds_byte_cap(
    checkpoints: &M11OrdinaryParagraphRestartCheckpoints,
    selection: M11OrdinaryParagraphBofCropSelection,
    target_suffix_start: usize,
) -> Result<bool, CandidateEndpointError> {
    if !selection.is_segmented_top_level() {
        return Ok(false);
    }
    let convergence = checkpoints
        .checkpoints()
        .get(selection.convergence_index())
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    if convergence.source() != selection.source()
        || convergence.binding() != selection.binding()
        || convergence.paragraph_source_start_byte() != selection.convergence_suffix_start_byte()
        || convergence.paragraph_source_start_utf16() != selection.convergence_suffix_start_utf16()
        || convergence.block_entry_ordinal() != selection.convergence_block_entry_ordinal()
    {
        return Err(CandidateEndpointError::InvalidAuthority);
    }
    let line_offset = usize::try_from(
        convergence
            .preceding_line_start_byte()
            .checked_sub(convergence.paragraph_source_start_byte())
            .ok_or(CandidateEndpointError::InvalidAuthority)?,
    )
    .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_crop_end = target_suffix_start
        .checked_add(line_offset)
        .and_then(|start| {
            start.checked_add(usize::try_from(convergence.preceding_line_physical_bytes()).ok()?)
        })
        .ok_or(CandidateEndpointError::MetricOverflow)?;
    Ok(target_crop_end > M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES)
}

pub(super) fn segmented_eof_crop_exceeds_byte_cap(
    checkpoints: &M11OrdinaryParagraphRestartCheckpoints,
    selection: M11OrdinaryParagraphEofCropSelection,
    target_crop_start: usize,
    target_eof: usize,
) -> Result<bool, CandidateEndpointError> {
    if !selection.is_segmented_top_level() {
        return Ok(false);
    }
    let restart = checkpoints
        .checkpoints()
        .get(selection.restart_index())
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    if restart.source() != selection.source()
        || restart.binding() != selection.binding()
        || restart.prefix_end_byte() != selection.restart_prefix_end_byte()
        || restart.prefix_end_utf16() != selection.restart_prefix_end_utf16()
        || restart.block_entry_ordinal() != selection.restart_block_entry_ordinal()
        || checkpoints.top_level_block_count() != selection.base_block_entry_count()
    {
        return Err(CandidateEndpointError::InvalidAuthority);
    }
    let target_crop_bytes = target_eof
        .checked_sub(target_crop_start)
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    Ok(target_crop_bytes > M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES)
}

pub(super) fn target_physical_line_cut_is_exact(
    target: &SourceSnapshotLease,
    byte_start: usize,
    utf16_start: usize,
) -> Result<bool, CandidateEndpointError> {
    let is_line_start = target
        .is_physical_line_start(byte_start)
        .map_err(|_| CandidateEndpointError::InvalidAuthority)?;
    let observed_utf16 = target
        .utf16_offset_for_byte(byte_start)
        .map_err(|_| CandidateEndpointError::InvalidAuthority)?;
    let observed_byte = target
        .byte_offset_for_utf16(utf16_start)
        .map_err(|_| CandidateEndpointError::InvalidAuthority)?;
    Ok(is_line_start && observed_utf16 == utf16_start && observed_byte == byte_start)
}

pub(super) fn leading_crop_declined_semantically(error: &M11LeadingReferencesCropError) -> bool {
    matches!(
        error,
        M11LeadingReferencesCropError::CropAcceptedDefinition
            | M11LeadingReferencesCropError::Unknown(_)
    )
}

pub(super) fn ordinary_crop_declined_semantically(error: &CandidateEndpointError) -> bool {
    matches!(
        error,
        CandidateEndpointError::OrdinaryCrop(M11OrdinaryParagraphCropError::CropDiverged)
            | CandidateEndpointError::OrdinaryBoundaryCrop(
                M11OrdinaryParagraphBoundaryCropError::CropDiverged
            )
    )
}

pub(super) fn take_candidate_restart_authority(
    result: &mut flark_parser::M11CleanDocumentResult,
    parser_binding: M11ParserBinding,
) -> Result<Option<CandidateRestartAuthority>, CandidateEndpointError> {
    match result.take_leading_references_restart_checkpoint(parser_binding) {
        Ok(restart) => Ok(Some(CandidateRestartAuthority::Leading(restart))),
        Err(LeadingReferencesCheckpointError::Ineligible) => {
            match result.take_ordinary_paragraph_restart_checkpoints(parser_binding) {
                Ok(restarts) => Ok(Some(CandidateRestartAuthority::Ordinary(restarts))),
                Err(M11OrdinaryParagraphCheckpointError::Ineligible) => {
                    Ok(Some(CandidateRestartAuthority::ExactBaseOnly {
                        source: result.source_version(),
                        binding: parser_binding,
                    }))
                }
                Err(M11OrdinaryParagraphCheckpointError::AllocationFailed) => {
                    Err(CandidateEndpointError::AllocationFailed)
                }
                Err(M11OrdinaryParagraphCheckpointError::AlreadyTaken) => {
                    Err(CandidateEndpointError::InvalidState)
                }
            }
        }
        Err(LeadingReferencesCheckpointError::AlreadyTaken) => {
            Err(CandidateEndpointError::InvalidState)
        }
    }
}

pub(super) fn plan_exact_clean_block_splice(
    runtime: &DocumentRuntime,
    base: &M11RetainedCandidatePublication,
    base_restart: Option<&CandidateRestartAuthority>,
    witness: &PersistentSourceFactsDeltaWitness,
    target: &M11CleanDocumentResult,
) -> Result<Option<M11BlockSequenceSpliceSelection>, CandidateEndpointError> {
    if target.source_version() != witness.target()
        || witness.base_byte_range().is_empty()
        || witness.target_byte_range().is_empty()
        || target.leaves().is_empty()
    {
        return Ok(None);
    }

    // `Before` at the changed-page start and `After` at its end deliberately
    // include neighboring leaves when either cut lands on a block boundary.
    // This trades a little transfer width for a simpler, fail-closed seam.
    let base_first = base
        .locate_exact_base_block_byte(
            runtime,
            witness.base_byte_range().start,
            SourceBoundaryAffinity::Before,
        )?
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    let base_last = base
        .locate_exact_base_block_byte(
            runtime,
            witness.base_byte_range().end,
            SourceBoundaryAffinity::After,
        )?
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    if base_first.entry_ordinal() > base_last.entry_ordinal()
        || base_first.byte_range().start
            > u64::try_from(witness.base_byte_range().start)
                .map_err(|_| CandidateEndpointError::MetricOverflow)?
        || base_last.byte_range().end
            < u64::try_from(witness.base_byte_range().end)
                .map_err(|_| CandidateEndpointError::MetricOverflow)?
    {
        return Ok(None);
    }
    let base_affected_entries = base_last
        .entry_ordinal()
        .checked_sub(base_first.entry_ordinal())
        .and_then(|count| count.checked_add(1))
        .ok_or(CandidateEndpointError::MetricOverflow)?;
    if base_affected_entries > EXACT_CLEAN_BLOCK_SPLICE_MAX_AFFECTED_ENTRIES {
        return Ok(None);
    }
    if base_first.byte_range().start == 0
        && base_last.byte_range().end
            == u64::try_from(witness.base().byte_len())
                .map_err(|_| CandidateEndpointError::MetricOverflow)?
    {
        // A whole-block-root replacement has no packed page to preserve.
        return Ok(None);
    }

    let Some(target_first_index) = clean_leaf_index_at(
        target.leaves(),
        witness.target_byte_range().start,
        witness.target().byte_len(),
        SourceBoundaryAffinity::Before,
    ) else {
        return Ok(None);
    };
    let Some(target_last_index) = clean_leaf_index_at(
        target.leaves(),
        witness.target_byte_range().end,
        witness.target().byte_len(),
        SourceBoundaryAffinity::After,
    ) else {
        return Ok(None);
    };
    if target_first_index > target_last_index {
        return Ok(None);
    }
    let target_affected_entries = target_last_index
        .checked_sub(target_first_index)
        .and_then(|count| count.checked_add(1))
        .ok_or(CandidateEndpointError::MetricOverflow)?;
    if u64::try_from(target_affected_entries).map_err(|_| CandidateEndpointError::MetricOverflow)?
        > EXACT_CLEAN_BLOCK_SPLICE_MAX_AFFECTED_ENTRIES
    {
        return Ok(None);
    }
    let target_first = &target.leaves()[target_first_index];
    let target_last = &target.leaves()[target_last_index];
    let target_first_source = target_first.source_range();
    let target_last_source = target_last.source_range();
    let target_first_byte = usize::try_from(target_first_source.start)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_last_byte = usize::try_from(target_last_source.end)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    if target_first_byte > witness.target_byte_range().start
        || target_last_byte < witness.target_byte_range().end
    {
        return Ok(None);
    }

    let target_start_ordinal =
        u64::try_from(target_first_index).map_err(|_| CandidateEndpointError::MetricOverflow)?;
    if base_first.entry_ordinal() != target_start_ordinal {
        return Ok(None);
    }

    let mut base_reference_definitions = 0_u64;
    let mut base_location = base_first.clone();
    loop {
        if base_location.entry().kind() == M11BlockSequenceEntryKind::Unsupported {
            return Ok(None);
        }
        base_reference_definitions = base_reference_definitions
            .checked_add(base_location.entry().reference_definition_count())
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        if base_location.entry_ordinal() == base_last.entry_ordinal() {
            break;
        }
        let next_byte = usize::try_from(base_location.byte_range().end)
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let next = base
            .locate_exact_base_block_byte(runtime, next_byte, SourceBoundaryAffinity::After)?
            .ok_or(CandidateEndpointError::InvalidAuthority)?;
        if next.entry_ordinal()
            != base_location
                .entry_ordinal()
                .checked_add(1)
                .ok_or(CandidateEndpointError::MetricOverflow)?
        {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        base_location = next;
    }
    if base_reference_definitions != 0
        || target.leaves()[target_first_index..=target_last_index]
            .iter()
            .any(|leaf| {
                leaf.reference_definition_count() != 0
                    || matches!(leaf, M11CleanLeaf::Unsupported { .. })
            })
    {
        return Ok(None);
    }
    if target.definition_count() != 0
        && !matches!(base_restart, Some(CandidateRestartAuthority::Leading(_)))
        && (witness.base().byte_len() != witness.target().byte_len()
            || witness.base().utf16_len() != witness.target().utf16_len())
    {
        // Canonical reference records currently own absolute byte and UTF-16
        // ranges. An unchanged suffix witness proves source identity, but a
        // nonzero coordinate delta still shifts every later definition. Only
        // a leading-reference checkpoint proves all retained definitions lie
        // in the exact unchanged prefix. Other definition-bearing documents
        // must rebuild References until the persistent reference index can
        // splice and rebase them explicitly.
        return Ok(None);
    }

    let base_prefix_end = usize::try_from(base_first.byte_range().start)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let base_prefix_utf16_end = usize::try_from(base_first.utf16_range().start)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_prefix_end = usize::try_from(target_first_source.start)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_prefix_utf16_end = usize::try_from(target_first.source_utf16_range().start)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    if base_prefix_end == 0 {
        if target_prefix_end != 0 || target_prefix_utf16_end != 0 {
            return Ok(None);
        }
    } else {
        let prefix = match runtime.mint_exact_unchanged_prefix_witness(
            witness.base(),
            base_prefix_end,
            base_prefix_utf16_end,
        ) {
            Ok(prefix) => prefix,
            Err(DocumentRuntimeError::ExactUnchangedPrefixLineageUnavailable) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let prefix = match runtime.take_exact_unchanged_prefix_witness(prefix) {
            Ok(prefix) => prefix,
            Err(DocumentRuntimeError::ExactUnchangedPrefixStale) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if prefix.target() != witness.target()
            || prefix.byte_end() != target_prefix_end
            || prefix.utf16_end() != target_prefix_utf16_end
        {
            return Ok(None);
        }
    }

    let base_suffix_start = usize::try_from(base_last.byte_range().end)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let base_suffix_utf16_start = usize::try_from(base_last.utf16_range().end)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_suffix_start = usize::try_from(target_last_source.end)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_suffix_utf16_start = usize::try_from(target_last.source_utf16_range().end)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    if base_suffix_start == witness.base().byte_len() {
        if target_suffix_start != witness.target().byte_len()
            || target_suffix_utf16_start != witness.target().utf16_len()
        {
            return Ok(None);
        }
    } else {
        let suffix = match runtime.mint_exact_unchanged_suffix_witness(
            witness.base(),
            base_suffix_start,
            base_suffix_utf16_start,
        ) {
            Ok(suffix) => suffix,
            Err(DocumentRuntimeError::ExactUnchangedSuffixLineageUnavailable) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let suffix = match runtime.take_exact_unchanged_suffix_witness(suffix) {
            Ok(suffix) => suffix,
            Err(DocumentRuntimeError::ExactUnchangedSuffixStale) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if suffix.target() != witness.target()
            || suffix.target_byte_start() != target_suffix_start
            || suffix.target_utf16_start() != target_suffix_utf16_start
        {
            return Ok(None);
        }
    }

    let base_end = base_last
        .entry_ordinal()
        .checked_add(1)
        .ok_or(CandidateEndpointError::MetricOverflow)?;
    let target_end = u64::try_from(
        target_last_index
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?,
    )
    .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let selection = M11BlockSequenceSpliceSelection::new(
        base_first.entry_ordinal()..base_end,
        target_start_ordinal..target_end,
    )
    .map_err(M11CandidateDerivationError::from)?;
    Ok(Some(selection))
}

fn clean_leaf_index_at(
    leaves: &[M11CleanLeaf],
    byte_offset: usize,
    source_bytes: usize,
    affinity: SourceBoundaryAffinity,
) -> Option<usize> {
    if leaves.is_empty() || source_bytes == 0 || byte_offset > source_bytes {
        return None;
    }
    let probe = match affinity {
        SourceBoundaryAffinity::Before if byte_offset > 0 => byte_offset - 1,
        SourceBoundaryAffinity::Before => 0,
        SourceBoundaryAffinity::After if byte_offset < source_bytes => byte_offset,
        SourceBoundaryAffinity::After => source_bytes - 1,
    };
    let probe = u32::try_from(probe).ok()?;
    let index = leaves.partition_point(|leaf| leaf.source_range().end <= probe);
    let range = leaves.get(index)?.source_range();
    (range.start <= probe && probe < range.end).then_some(index)
}

pub(super) fn begin_exact_clean_fallback(
    runtime: &DocumentRuntime,
    context: CandidateContext,
    base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
) -> Result<ActiveCandidate, ExactBuildStartFailure> {
    let certified = match runtime.certify_current_persistent_source() {
        Ok(certified) => certified,
        Err(error) => {
            return Err(ExactBuildStartFailure {
                error: error.into(),
                base,
            });
        }
    };
    if certified.source() != witness.target()
        || certified.parser_profile() != witness.parser_profile()
        || certified.source_facts_profile() != witness.profile()
    {
        return Err(ExactBuildStartFailure {
            error: CandidateEndpointError::InvalidAuthority,
            base,
        });
    }
    let job = match M11CleanParseJob::new(certified.exact_parse_lease()) {
        Ok(job) => job,
        Err(error) => {
            return Err(ExactBuildStartFailure {
                error: error.into(),
                base,
            });
        }
    };
    Ok(ActiveCandidate::ParsingExactFallback(Box::new(
        ParsingExactFallbackCandidate {
            context,
            certified,
            job,
            base,
            witness,
        },
    )))
}

pub(super) fn begin_exact_candidate_build_ordinary(
    runtime: &mut DocumentRuntime,
    context: CandidateContext,
    mut base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
    mut result: OrdinaryExactResult,
) -> Result<ActiveCandidate, ExactBuildStartFailure> {
    let base_restart = match result.take_base_restart_checkpoints() {
        Ok(checkpoints) => checkpoints,
        Err(_) => {
            return Err(ExactBuildStartFailure {
                error: CandidateEndpointError::InvalidState,
                base,
            });
        }
    };
    base.restart = Some(CandidateRestartAuthority::Ordinary(base_restart));
    let next_restart = match result.take_next_restart_checkpoints() {
        Ok(checkpoints) => checkpoints,
        Err(_) => {
            return Err(ExactBuildStartFailure {
                error: CandidateEndpointError::InvalidState,
                base,
            });
        }
    };
    let input = match result.into_exact_segmented_candidate_input() {
        Ok(input) => input,
        Err(error) => {
            return Err(ExactBuildStartFailure {
                error: error.into(),
                base,
            });
        }
    };
    let next_restart = CandidateRestartAuthority::Ordinary(next_restart);
    begin_exact_candidate_build_from_terminal(runtime, context, base, witness, input, next_restart)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn begin_exact_candidate_build_from_terminal(
    runtime: &mut DocumentRuntime,
    context: CandidateContext,
    base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
    input: M11ExactSegmentedCandidateInput,
    next_restart: CandidateRestartAuthority,
) -> Result<ActiveCandidate, ExactBuildStartFailure> {
    if input.source() != witness.target() {
        return Err(ExactBuildStartFailure {
            error: CandidateEndpointError::InvalidAuthority,
            base,
        });
    }
    let candidate = match M11ParserCandidate::derive_segmented_reusing_references(
        input,
        witness.parser_profile(),
        witness.profile(),
    ) {
        Ok(candidate) => candidate,
        Err(error) => {
            return Err(ExactBuildStartFailure {
                error: error.into(),
                base,
            });
        }
    };
    let publication = derive_identity(
        b"publication",
        context.binding,
        context.completion,
        context.parse_generation,
    );
    let writer = match candidate.into_writer(
        runtime,
        document_bytes(context.binding.document_session),
        publication,
        u64::from(context.parse_generation),
    ) {
        Ok(writer) => writer,
        Err(error) => {
            return Err(ExactBuildStartFailure {
                error: error.into(),
                base,
            });
        }
    };
    Ok(ActiveCandidate::BuildingExact {
        context,
        writer: Box::new(writer),
        base,
        witness,
        next_restart,
        structural_path: ExactStructuralPath::LegacyBlocks,
    })
}

pub(super) fn offer_begin(
    context: CandidateContext,
    descriptor: M11CandidateDescriptor,
) -> Result<OfferBegin, CandidateEndpointError> {
    offer_begin_with_mode(
        context,
        descriptor,
        PublicationMode::FullSnapshot,
        None,
        descriptor.canonical_record_count,
    )
}

pub(super) fn offer_begin_exact(
    context: CandidateContext,
    descriptor: M11CandidateDescriptor,
    transferred_record_count: u64,
    base_ack: StructuralAck,
) -> Result<OfferBegin, CandidateEndpointError> {
    offer_begin_with_mode(
        context,
        descriptor,
        PublicationMode::ExactBaseDelta,
        Some(base_ack),
        transferred_record_count,
    )
}

fn offer_begin_with_mode(
    context: CandidateContext,
    descriptor: M11CandidateDescriptor,
    mode: PublicationMode,
    base_ack: Option<StructuralAck>,
    transferred_record_count: u64,
) -> Result<OfferBegin, CandidateEndpointError> {
    if descriptor.document != document_bytes(context.binding.document_session)
        || descriptor.source_revision != u64::from(context.completion.worker_replica_revision)
        || descriptor.source_bytes != u64::from(context.completion.utf8_length)
        || descriptor.source_utf16 != u64::from(context.completion.utf16_length)
        || descriptor.parse_generation != u64::from(context.parse_generation)
        || descriptor.syntax_profile == 0
    {
        return Err(CandidateEndpointError::InvalidAuthority);
    }
    let target_record_count = u32::try_from(descriptor.canonical_record_count)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let transferred_record_count = u32::try_from(transferred_record_count)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let maximum_frame_count = u32::try_from(descriptor.maximum_snapshot_frames)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let maximum_encoded_frame_bytes = u32::try_from(descriptor.maximum_snapshot_encoded_bytes)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let source_version = SourceVersion {
        document_session: context.binding.document_session,
        revision: context.completion.ui_revision,
        utf8_length: context.completion.utf8_length,
        utf16_length: context.completion.utf16_length,
        content_hash128: context.completion.content_hash128,
    };
    Ok(OfferBegin {
        schema: MANIFEST_SCHEMA,
        offer_id: digest_words(derive_identity(
            b"offer",
            context.binding,
            context.completion,
            context.parse_generation,
        )),
        publication_session: digest_words(descriptor.publication),
        target_host_revision: context.parse_generation,
        source_version,
        source_root: split_u64(descriptor.source_root),
        parse_generation: context.parse_generation,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: descriptor.syntax_profile,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        mode,
        base_ack,
        transferred_record_count,
        target_record_count,
        limits: OfferLimits {
            maximum_frame_count,
            maximum_encoded_frame_bytes,
            maximum_packet_bytes: u32::try_from(MAXIMUM_PACKET_ENCODED_BYTES)
                .map_err(|_| CandidateEndpointError::MetricOverflow)?,
            // A protocol record is carried inside a complete snapshot Node
            // frame, so this ceiling includes the engine frame header.
            maximum_frame_bytes: u32::try_from(M11_MAX_SNAPSHOT_FRAME_BYTES)
                .map_err(|_| CandidateEndpointError::MetricOverflow)?,
            maximum_program_children: u32::try_from(M11_MAX_ROLE_RECORDS)
                .map_err(|_| CandidateEndpointError::MetricOverflow)?,
        },
    })
}

impl StreamingCandidate {
    pub(super) fn poll_event(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        match self.phase {
            StreamPhase::NeedBegin => {
                self.phase = StreamPhase::AwaitBeginReceipt;
                Ok(CandidatePoll::Event {
                    transitions: 0,
                    event: Box::new(CandidateEvent {
                        credit: CandidateCredit::Begin,
                        body: CandidateEventBody::Begin(self.offer),
                    }),
                })
            }
            StreamPhase::NeedPacket => self.poll_packet(runtime, fuel),
            StreamPhase::NeedCommit => {
                let commit = self.commit.ok_or(CandidateEndpointError::InvalidState)?;
                self.phase = StreamPhase::AwaitCommitReceipt;
                Ok(CandidatePoll::Event {
                    transitions: 0,
                    event: Box::new(CandidateEvent {
                        credit: CandidateCredit::Commit,
                        body: CandidateEventBody::Commit(commit),
                    }),
                })
            }
            _ => Ok(CandidatePoll::Pending { transitions: 0 }),
        }
    }

    fn poll_packet(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        let maximum_packet_bytes = usize::try_from(self.offer.limits.maximum_packet_bytes)
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let mut transitions = 0_usize;
        loop {
            if self.packet.end || self.packet.saturated(maximum_packet_bytes)? {
                let event = self.take_packet_event()?;
                return Ok(CandidatePoll::Event {
                    transitions,
                    event: Box::new(event),
                });
            }
            let polled = if let Some(frame) = self.lookahead.take() {
                M11OwnedSnapshotPoll::Frame {
                    transitions: 0,
                    frame,
                }
            } else if transitions == fuel {
                return Ok(CandidatePoll::Pending { transitions });
            } else {
                let stream = self
                    .stream
                    .as_mut()
                    .ok_or(CandidateEndpointError::InvalidState)?;
                if self.next_frame_ordinal == 0 {
                    if !self.packet.frames.is_empty() {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    M11OwnedSnapshotPoll::Frame {
                        transitions: 0,
                        frame: stream.begin_frame()?,
                    }
                } else {
                    stream.poll(runtime, fuel - transitions)?
                }
            };
            match polled {
                M11OwnedSnapshotPoll::Pending {
                    transitions: consumed,
                } => {
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    return Ok(CandidatePoll::Pending { transitions });
                }
                M11OwnedSnapshotPoll::Frame {
                    transitions: consumed,
                    frame,
                } => {
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    if !self
                        .packet
                        .can_accept(frame.bytes.len(), maximum_packet_bytes)?
                    {
                        if self.packet.frames.is_empty() || self.lookahead.is_some() {
                            return Err(CandidateEndpointError::InvalidState);
                        }
                        self.lookahead = Some(frame);
                        let event = self.take_packet_event()?;
                        return Ok(CandidatePoll::Event {
                            transitions,
                            event: Box::new(event),
                        });
                    }
                    self.append_frame(runtime, frame)?;
                    if self.packet.end || self.packet.saturated(maximum_packet_bytes)? {
                        let event = self.take_packet_event()?;
                        return Ok(CandidatePoll::Event {
                            transitions,
                            event: Box::new(event),
                        });
                    }
                    if transitions == fuel {
                        return Ok(CandidatePoll::Pending { transitions });
                    }
                }
                M11OwnedSnapshotPoll::ReplayRequired {
                    transitions: consumed,
                } => {
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel || self.resume_after_packet_credit {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    if self.packet.frames.is_empty() {
                        // The final replacement page may have exactly filled a
                        // prior packet. Reaching this branch means that packet
                        // has already received host credit, so its replay is
                        // complete and the producer can resume immediately.
                        self.stream
                            .as_mut()
                            .ok_or(CandidateEndpointError::InvalidState)?
                            .resume_exact_base_delta()?;
                        continue;
                    }
                    self.resume_after_packet_credit = true;
                    let event = self.take_packet_event()?;
                    return Ok(CandidatePoll::Event {
                        transitions,
                        event: Box::new(event),
                    });
                }
            }
        }
    }

    fn append_frame(
        &mut self,
        runtime: &DocumentRuntime,
        frame: M11SnapshotFrame,
    ) -> Result<(), CandidateEndpointError> {
        let ordinal = self.next_frame_ordinal;
        let first_record_ordinal = self.next_record_ordinal;
        let wire_kind = match frame.kind {
            M11SnapshotFrameKind::Begin => CandidateSnapshotFrameKind::Begin,
            M11SnapshotFrameKind::Node => CandidateSnapshotFrameKind::Node,
            M11SnapshotFrameKind::End => CandidateSnapshotFrameKind::End,
            M11SnapshotFrameKind::SourceFactsReplacementPage => {
                CandidateSnapshotFrameKind::SourceFactsReplacementPage
            }
            M11SnapshotFrameKind::BlockSequenceReplacementPage => {
                CandidateSnapshotFrameKind::BlockSequenceReplacementPage
            }
            M11SnapshotFrameKind::RecursiveGreenReplacementPage => {
                CandidateSnapshotFrameKind::RecursiveGreenReplacementPage
            }
        };
        if matches!(frame.kind, M11SnapshotFrameKind::Begin) != (ordinal == 0)
            || frame.bytes.is_empty()
            || frame.bytes.len() > M11_MAX_SNAPSHOT_FRAME_BYTES
        {
            return Err(CandidateEndpointError::InvalidState);
        }
        if frame.kind == M11SnapshotFrameKind::Node {
            let node_ordinal = frame
                .node_ordinal
                .ok_or(CandidateEndpointError::InvalidState)?;
            if self
                .next_node_ordinal
                .is_some_and(|expected| node_ordinal != expected)
            {
                return Err(CandidateEndpointError::InvalidState);
            }
            self.next_node_ordinal = Some(
                node_ordinal
                    .checked_add(1)
                    .ok_or(CandidateEndpointError::MetricOverflow)?,
            );
        } else if frame.node_ordinal.is_some() {
            return Err(CandidateEndpointError::InvalidState);
        }
        let next_frame_ordinal = ordinal
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let next_record_ordinal = first_record_ordinal
            .checked_add(frame.canonical_record_count)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let digest256 = self
            .transport
            .as_mut()
            .ok_or(CandidateEndpointError::InvalidState)?
            .push(
                ordinal,
                first_record_ordinal,
                frame.canonical_record_count,
                wire_kind,
                &frame.bytes,
            )?;
        let end = frame.kind == M11SnapshotFrameKind::End;
        let canonical_digest = frame.canonical_stream_digest256;
        self.packet.push(
            ordinal,
            first_record_ordinal,
            frame.canonical_record_count,
            protocol_digest128_from_blake3(ProtocolDigestDomain::CandidateFrame, digest256),
            frame.bytes,
            end,
        )?;
        self.next_frame_ordinal = next_frame_ordinal;
        self.next_record_ordinal = next_record_ordinal;
        if end {
            self.finish_frame_stream(runtime, canonical_digest)?;
        }
        Ok(())
    }

    fn finish_frame_stream(
        &mut self,
        runtime: &DocumentRuntime,
        canonical_digest: Option<[u8; 32]>,
    ) -> Result<(), CandidateEndpointError> {
        if self.next_record_ordinal != self.offer.transferred_record_count {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        if self.sealed_publication.is_some() {
            return Err(CandidateEndpointError::InvalidState);
        }
        let canonical_stream_digest = protocol_digest128_from_blake3(
            ProtocolDigestDomain::CandidateStream,
            canonical_digest.ok_or(CandidateEndpointError::InvalidState)?,
        );
        let mut stream = self
            .stream
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        match self.offer.mode {
            PublicationMode::FullSnapshot | PublicationMode::ExactBaseReferencesDelta => {}
            PublicationMode::ExactBaseDelta => {
                if self.superseded_exact_base.is_some() {
                    self.stream = Some(stream);
                    return Err(CandidateEndpointError::InvalidState);
                }
                let superseded_exact_base = match stream.take_superseded_exact_base(runtime) {
                    Ok(Some(base)) => base,
                    Ok(None) => {
                        self.stream = Some(stream);
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    Err(error) => {
                        self.stream = Some(stream);
                        return Err(error.into());
                    }
                };
                self.superseded_exact_base = Some(Box::new(superseded_exact_base));
            }
        }
        let publication = match stream.into_retained_publication(runtime) {
            Ok(publication) => publication,
            Err(failure) => {
                let (error, stream) = failure.into_parts();
                self.stream = Some(stream);
                return Err(error.into());
            }
        };
        self.sealed_publication = Some(publication);
        self.canonical_stream_digest = Some(canonical_stream_digest);
        let transport = self
            .transport
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?
            .finish();
        if transport.frame_count != self.next_frame_ordinal {
            return Err(CandidateEndpointError::InvalidState);
        }
        self.commit = Some(CommitRequest {
            offer_id: self.offer.offer_id,
            actual_frame_count: transport.frame_count,
            actual_encoded_frame_bytes: transport.encoded_frame_bytes,
            rolling_transport_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::CandidateTransport,
                transport.digest256,
            ),
            canonical_stream_digest,
        });
        self.expected_ack = Some(StructuralAck {
            publication_session: self.offer.publication_session,
            host_revision: self.offer.target_host_revision,
            source_version: self.offer.source_version,
            source_root: self.offer.source_root,
            parse_generation: self.offer.parse_generation,
            grammar_revision: self.offer.grammar_revision,
            syntax_profile: self.offer.syntax_profile,
            authority_mask: self.offer.authority_mask,
            record_count: self.offer.target_record_count,
            sequence_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::CandidateAckSequence,
                self.descriptor.manifest_digest256,
            ),
            manifest_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::CandidateManifest,
                self.descriptor.manifest_digest256,
            ),
        });
        Ok(())
    }

    fn take_packet_event(&mut self) -> Result<CandidateEvent, CandidateEndpointError> {
        let packet = std::mem::take(&mut self.packet);
        let first_frame_ordinal = packet
            .first_frame_ordinal
            .ok_or(CandidateEndpointError::InvalidState)?;
        let frame_count = u32::try_from(packet.frames.len())
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let end = packet.end;
        let encoded = packet.encode(self.offer.offer_id)?;
        self.phase = StreamPhase::AwaitPacketReceipt {
            first_frame_ordinal,
            frame_count,
            end,
        };
        Ok(CandidateEvent {
            credit: CandidateCredit::Packet {
                first_frame_ordinal,
                frame_count,
                end,
            },
            body: CandidateEventBody::Packet { encoded },
        })
    }
}
