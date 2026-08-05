use std::collections::BTreeSet;
use std::ops::Range;

use flark_v3_runtime_slice::{
    AtomicProjection, LogicalContribution, ProjectionChunk, ProjectionChunkerFinish,
    ProjectionPiece, ProjectionProgramChunker, SerializedMetric,
};

#[derive(Debug)]
struct SpannedChunk {
    physical: Range<u64>,
    chunk: ProjectionChunk,
}

fn dense_nul_chunks(physical_bytes: u64) -> Vec<SpannedChunk> {
    let metric = SerializedMetric {
        bytes: physical_bytes,
        utf16: physical_bytes,
    };
    let mut chunker = ProjectionProgramChunker::new(metric).unwrap();
    let mut chunks = Vec::new();

    for _ in 0..physical_bytes {
        if let Some(chunk) = chunker
            .push(ProjectionPiece::Atomic {
                physical_metric: SerializedMetric { bytes: 1, utf16: 1 },
                projection: AtomicProjection::nul_to_replacement(),
            })
            .unwrap()
        {
            chunks.push(chunk);
        }
    }
    loop {
        let (chunk, status) = chunker.finish().unwrap();
        if let Some(chunk) = chunk {
            chunks.push(chunk);
        }
        if matches!(status, ProjectionChunkerFinish::Complete(_)) {
            break;
        }
    }

    let mut physical = 0_u64;
    chunks
        .into_iter()
        .map(|chunk| {
            let start = physical;
            physical += chunk.physical_metric.bytes;
            SpannedChunk {
                physical: start..physical,
                chunk,
            }
        })
        .collect()
}

fn dense_nul_chunks_at_stable_resets(ranges: &[Range<u64>]) -> Vec<SpannedChunk> {
    ranges
        .iter()
        .flat_map(|range| {
            let start = range.start;
            dense_nul_chunks(range.end - range.start)
                .into_iter()
                .map(move |mut chunk| {
                    chunk.physical.start += start;
                    chunk.physical.end += start;
                    chunk
                })
        })
        .collect()
}

fn map_old_range_after_insert(old: &Range<u64>, at: u64, inserted: u64) -> Option<Range<u64>> {
    if old.end <= at {
        Some(old.clone())
    } else if at <= old.start {
        Some(old.start + inserted..old.end + inserted)
    } else {
        None
    }
}

fn same_reusable_projection(old: &SpannedChunk, new: &SpannedChunk) -> bool {
    old.chunk.physical_metric == new.chunk.physical_metric
        && old.chunk.logical_contribution == new.chunk.logical_contribution
}

fn eligible_exact_pages(
    old: &[SpannedChunk],
    new: &[SpannedChunk],
    insert_at: u64,
    inserted_bytes: u64,
) -> usize {
    old.iter()
        .filter_map(|old_chunk| {
            let mapped =
                map_old_range_after_insert(&old_chunk.physical, insert_at, inserted_bytes)?;
            new.iter()
                .find(|new_chunk| new_chunk.physical == mapped)
                .map(|new_chunk| (old_chunk, new_chunk))
        })
        .filter(|(old_chunk, new_chunk)| same_reusable_projection(old_chunk, new_chunk))
        .count()
}

/// Characterization falsifier: canonical max-fill pages are deterministic for
/// one source, but a one-byte insertion can move every source-relative page
/// boundary in one giant transformed envelope. Exact ArenaId/CoverageId suffix
/// reuse cannot exceed the eligible page count measured here.
#[test]
fn one_local_insert_churns_the_entire_dense_envelope_suffix() {
    const OLD_BYTES: u64 = 200_000;
    const INSERT_AT: u64 = 1;
    const INSERTED_BYTES: u64 = 1;

    let old = dense_nul_chunks(OLD_BYTES);
    let new = dense_nul_chunks(OLD_BYTES + INSERTED_BYTES);
    assert!(
        old.len() > 100,
        "the falsifier must span many Program pages"
    );

    let new_internal_boundaries = new
        .iter()
        .take(new.len() - 1)
        .map(|chunk| chunk.physical.end)
        .collect::<BTreeSet<_>>();
    let aligned_internal_boundaries = old
        .iter()
        .take(old.len() - 1)
        .filter_map(|chunk| {
            map_old_range_after_insert(&chunk.physical, INSERT_AT, INSERTED_BYTES)
                .map(|mapped| mapped.end)
        })
        .filter(|boundary| new_internal_boundaries.contains(boundary))
        .count();

    let eligible_exact_pages = eligible_exact_pages(&old, &new, INSERT_AT, INSERTED_BYTES);

    eprintln!(
        "old_pages={} new_pages={} mapped_internal_boundaries={} eligible_exact_pages={}",
        old.len(),
        new.len(),
        aligned_internal_boundaries,
        eligible_exact_pages
    );

    assert_eq!(
        aligned_internal_boundaries, 0,
        "greedy fill unexpectedly found a stable internal reset boundary"
    );
    assert_eq!(
        eligible_exact_pages, 0,
        "an exact source-mapped Program page unexpectedly survived the insertion"
    );
    assert!(old.iter().all(|chunk| matches!(
        chunk.chunk.logical_contribution,
        LogicalContribution::Program(_)
    )));
}

/// Comparison model, not a production implementation: source-store anchors
/// that survive the edit provide deterministic reset points. Re-running greedy
/// packing independently after each reset bounds cascade to the first reset
/// interval touched by the edit.
#[test]
fn stable_source_reset_model_recovers_the_untouched_dense_suffix() {
    const OLD_BYTES: u64 = 200_000;
    const RESET_BYTES: u64 = 8 * 1024;
    const INSERT_AT: u64 = 1;
    const INSERTED_BYTES: u64 = 1;

    let mut old_resets = Vec::new();
    let mut start = 0;
    while start < OLD_BYTES {
        let end = (start + RESET_BYTES).min(OLD_BYTES);
        old_resets.push(start..end);
        start = end;
    }
    let old = dense_nul_chunks_at_stable_resets(&old_resets);

    let mut new_boundaries = vec![0];
    new_boundaries.extend(
        old_resets
            .iter()
            .map(|range| range.end)
            .filter(|boundary| *boundary < OLD_BYTES)
            .map(|boundary| boundary + INSERTED_BYTES),
    );
    new_boundaries.push(OLD_BYTES + INSERTED_BYTES);
    let new_resets = new_boundaries
        .windows(2)
        .map(|pair| pair[0]..pair[1])
        .collect::<Vec<_>>();
    let new = dense_nul_chunks_at_stable_resets(&new_resets);

    let reusable = eligible_exact_pages(&old, &new, INSERT_AT, INSERTED_BYTES);
    eprintln!(
        "reset_model old_pages={} new_pages={} eligible_exact_pages={}",
        old.len(),
        new.len(),
        reusable
    );
    assert!(
        reusable * 10 > old.len() * 9,
        "stable reset model should preserve more than 90% of dense suffix pages"
    );
}
