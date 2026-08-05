use std::collections::BTreeSet;
use std::ops::Range;

use flark_v3_runtime_slice::{
    AcceptedEdit, AtomicProjection, BoundaryAffinity, LogicalContribution, MapStatus,
    ProjectionChunk, ProjectionChunkerFinish, ProjectionPiece, ProjectionProgramChunker,
    ProvenSourceMapping, SerializedMetric, SourceRevision, SourceRootId, SourceStore,
};

const TARGET_GROUP_BYTES: usize = 8 * 1024;
const MIN_GROUP_BYTES: usize = 4 * 1024;
const MAX_GROUP_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug)]
struct ResetCapability {
    id: u64,
    revision: SourceRevision,
    offset: usize,
    affinity: BoundaryAffinity,
}

#[derive(Clone, Debug)]
struct ProjectionPage {
    source: Range<usize>,
    chunk: ProjectionChunk,
}

#[derive(Clone, Debug)]
struct ProjectionGroup {
    source: Range<usize>,
    pages: Vec<ProjectionPage>,
}

#[derive(Clone, Debug)]
struct ProjectionLayout {
    revision: SourceRevision,
    root: SourceRootId,
    source_bytes: usize,
    source_utf16: usize,
    resets: Vec<ResetCapability>,
    groups: Vec<ProjectionGroup>,
    next_reset_id: u64,
}

impl ProjectionLayout {
    fn page_count(&self) -> usize {
        self.groups.iter().map(|group| group.pages.len()).sum()
    }

    fn pages(&self) -> impl Iterator<Item = &ProjectionPage> {
        self.groups.iter().flat_map(|group| group.pages.iter())
    }

    fn maximum_group_bytes(&self) -> usize {
        self.groups
            .iter()
            .map(|group| group.source.len())
            .max()
            .unwrap_or(0)
    }

    fn maximum_group_pages(&self) -> usize {
        self.groups
            .iter()
            .map(|group| group.pages.len())
            .max()
            .unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ResetReceipt {
    mapped: usize,
    changed: usize,
    dropped_illegal: usize,
    dropped_duplicate: usize,
    removed_underflow: usize,
    minted: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct ReuseReceipt {
    old_pages: usize,
    new_pages: usize,
    source_mapped_old_pages: usize,
    exact_reusable_pages: usize,
    source_mapped_suffix_pages: usize,
    exact_reusable_suffix_pages: usize,
    changed_new_pages: usize,
    changed_new_groups: usize,
    maximum_changed_group_pages: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct TransitionReceipt {
    resets: ResetReceipt,
    reuse: ReuseReceipt,
    maximum_group_bytes: usize,
    maximum_group_pages: usize,
}

fn source_metric(text: &str) -> SerializedMetric {
    SerializedMetric {
        bytes: u64::try_from(text.len()).expect("test source length fits u64"),
        utf16: u64::try_from(text.encode_utf16().count()).expect("test UTF-16 length fits u64"),
    }
}

fn is_legal_projection_boundary(text: &str, offset: usize) -> bool {
    if !text.is_char_boundary(offset) {
        return false;
    }
    if offset == 0 || offset == text.len() {
        return true;
    }
    let bytes = text.as_bytes();
    !(bytes[offset - 1] == b'\r' && bytes[offset] == b'\n')
}

fn choose_split(text: &str, source: &Range<usize>) -> usize {
    debug_assert!(source.len() > MAX_GROUP_BYTES);
    let lower = source.start + MIN_GROUP_BYTES;
    let upper = (source.start + MAX_GROUP_BYTES).min(source.end - MIN_GROUP_BYTES);
    let desired = (source.start + TARGET_GROUP_BYTES).clamp(lower, upper);
    text.char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
        .filter(|offset| (lower..=upper).contains(offset))
        .filter(|offset| is_legal_projection_boundary(text, *offset))
        .min_by_key(|offset| offset.abs_diff(desired))
        .expect("a bounded UTF-8 scalar/CRLF projection has a legal split")
}

fn deduplicate_resets(resets: &mut Vec<ResetCapability>, receipt: &mut ResetReceipt) {
    resets.sort_by_key(|reset| (reset.offset, reset.id));
    let before = resets.len();
    resets.dedup_by_key(|reset| reset.offset);
    receipt.dropped_duplicate += before - resets.len();
}

fn rebalance_resets(
    text: &str,
    revision: SourceRevision,
    resets: &mut Vec<ResetCapability>,
    next_reset_id: &mut u64,
    default_affinity: BoundaryAffinity,
    receipt: &mut ResetReceipt,
) {
    let before = resets.len();
    resets.retain(|reset| {
        reset.revision == revision
            && reset.offset > 0
            && reset.offset < text.len()
            && is_legal_projection_boundary(text, reset.offset)
    });
    receipt.dropped_illegal += before - resets.len();
    deduplicate_resets(resets, receipt);

    loop {
        let boundaries = std::iter::once(0)
            .chain(resets.iter().map(|reset| reset.offset))
            .chain(std::iter::once(text.len()))
            .collect::<Vec<_>>();
        let group_count = boundaries.len() - 1;

        if group_count > 1
            && let Some(group_index) = boundaries
                .windows(2)
                .position(|pair| pair[1] - pair[0] < MIN_GROUP_BYTES)
        {
            let reset_index = if group_index == 0 { 0 } else { group_index - 1 };
            resets.remove(reset_index);
            receipt.removed_underflow += 1;
            continue;
        }

        if let Some(pair) = boundaries
            .windows(2)
            .find(|pair| pair[1] - pair[0] > MAX_GROUP_BYTES)
        {
            let source = pair[0]..pair[1];
            let offset = choose_split(text, &source);
            let id = *next_reset_id;
            *next_reset_id = next_reset_id
                .checked_add(1)
                .expect("test reset identity space is ample");
            resets.push(ResetCapability {
                id,
                revision,
                offset,
                affinity: default_affinity,
            });
            receipt.minted += 1;
            deduplicate_resets(resets, receipt);
            continue;
        }
        break;
    }
}

fn push_piece(
    chunker: &mut ProjectionProgramChunker,
    chunks: &mut Vec<ProjectionChunk>,
    piece: ProjectionPiece,
) {
    if let Some(chunk) = chunker
        .push(piece)
        .expect("source-derived projection piece")
    {
        chunks.push(chunk);
    }
}

fn push_source_projection(
    text: &str,
    chunker: &mut ProjectionProgramChunker,
) -> Vec<ProjectionChunk> {
    let bytes = text.as_bytes();
    let mut chunks = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let (piece, consumed) = match bytes[offset] {
            b'\0' => (
                ProjectionPiece::Atomic {
                    physical_metric: SerializedMetric { bytes: 1, utf16: 1 },
                    projection: AtomicProjection::nul_to_replacement(),
                },
                1,
            ),
            b'\t' => (
                ProjectionPiece::Atomic {
                    physical_metric: SerializedMetric { bytes: 1, utf16: 1 },
                    projection: AtomicProjection::tab_to_spaces(4).expect("typed tab transform"),
                },
                1,
            ),
            b'\r' if bytes.get(offset + 1) == Some(&b'\n') => (
                ProjectionPiece::Atomic {
                    physical_metric: SerializedMetric { bytes: 2, utf16: 2 },
                    projection: AtomicProjection::crlf_to_lf(),
                },
                2,
            ),
            b'\r' => (
                ProjectionPiece::Atomic {
                    physical_metric: SerializedMetric { bytes: 1, utf16: 1 },
                    projection: AtomicProjection::lone_cr_to_lf(),
                },
                1,
            ),
            _ => {
                let scalar = text[offset..]
                    .chars()
                    .next()
                    .expect("offset is a scalar boundary");
                let consumed = scalar.len_utf8();
                (
                    ProjectionPiece::Identity {
                        metric: SerializedMetric {
                            bytes: u64::try_from(consumed).expect("scalar bytes fit u64"),
                            utf16: u64::try_from(scalar.len_utf16())
                                .expect("scalar UTF-16 length fits u64"),
                        },
                    },
                    consumed,
                )
            }
        };
        push_piece(chunker, &mut chunks, piece);
        offset += consumed;
    }

    loop {
        let (chunk, status) = chunker.finish().expect("complete source envelope");
        if let Some(chunk) = chunk {
            chunks.push(chunk);
        }
        if matches!(status, ProjectionChunkerFinish::Complete(_)) {
            break;
        }
    }
    chunks
}

fn build_group(text: &str, source: Range<usize>) -> ProjectionGroup {
    assert!(is_legal_projection_boundary(text, source.start));
    assert!(is_legal_projection_boundary(text, source.end));
    let slice = &text[source.clone()];
    let expected = source_metric(slice);
    let mut chunker = ProjectionProgramChunker::new(expected).expect("nonempty source group");
    let chunks = push_source_projection(slice, &mut chunker);
    let mut byte_offset = source.start;
    let mut utf16 = 0_u64;
    let pages = chunks
        .into_iter()
        .map(|chunk| {
            let start = byte_offset;
            byte_offset += usize::try_from(chunk.physical_metric.bytes)
                .expect("test chunk byte metric fits usize");
            utf16 += chunk.physical_metric.utf16;
            ProjectionPage {
                source: start..byte_offset,
                chunk,
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(byte_offset, source.end);
    assert_eq!(utf16, expected.utf16);
    assert!(pages.iter().all(|page| {
        is_legal_projection_boundary(text, page.source.start)
            && is_legal_projection_boundary(text, page.source.end)
    }));
    ProjectionGroup { source, pages }
}

fn build_layout(
    store: &SourceStore,
    resets: Vec<ResetCapability>,
    next_reset_id: u64,
) -> ProjectionLayout {
    let snapshot = store.query_snapshot();
    let revision = snapshot.revision();
    let root = snapshot.identity();
    let text = snapshot.materialize_for_testing();
    let metric = source_metric(&text);
    let boundaries = std::iter::once(0)
        .chain(resets.iter().map(|reset| reset.offset))
        .chain(std::iter::once(text.len()))
        .collect::<Vec<_>>();
    let groups = boundaries
        .windows(2)
        .map(|pair| build_group(&text, pair[0]..pair[1]))
        .collect::<Vec<_>>();
    let layout = ProjectionLayout {
        revision,
        root,
        source_bytes: text.len(),
        source_utf16: usize::try_from(metric.utf16).expect("test UTF-16 length fits usize"),
        resets,
        groups,
        next_reset_id,
    };
    assert_layout_invariants(&layout, &text);
    layout
}

fn initial_layout(store: &SourceStore, affinity: BoundaryAffinity) -> ProjectionLayout {
    let snapshot = store.query_snapshot();
    let text = snapshot.materialize_for_testing();
    let mut resets = Vec::new();
    let mut next_reset_id = 1;
    let mut receipt = ResetReceipt::default();
    rebalance_resets(
        &text,
        snapshot.revision(),
        &mut resets,
        &mut next_reset_id,
        affinity,
        &mut receipt,
    );
    assert_eq!(receipt.mapped, 0);
    assert_eq!(receipt.changed, 0);
    build_layout(store, resets, next_reset_id)
}

fn assert_layout_invariants(layout: &ProjectionLayout, text: &str) {
    assert_eq!(layout.source_bytes, text.len());
    assert_eq!(layout.source_utf16, text.encode_utf16().count());
    assert!(
        layout
            .resets
            .windows(2)
            .all(|pair| pair[0].offset < pair[1].offset)
    );
    assert!(layout.resets.iter().all(|reset| {
        reset.revision == layout.revision
            && is_legal_projection_boundary(text, reset.offset)
            && reset.offset > 0
            && reset.offset < text.len()
    }));
    assert!(
        layout
            .groups
            .iter()
            .all(|group| group.source.len() <= MAX_GROUP_BYTES)
    );
    if layout.groups.len() > 1 {
        assert!(
            layout
                .groups
                .iter()
                .all(|group| group.source.len() >= MIN_GROUP_BYTES)
        );
    }
    assert_eq!(
        layout
            .groups
            .iter()
            .map(|group| group.source.len())
            .sum::<usize>(),
        text.len()
    );
    assert_eq!(
        layout
            .pages()
            .map(|page| usize::try_from(page.chunk.physical_metric.bytes).unwrap())
            .sum::<usize>(),
        text.len()
    );
    assert_eq!(
        layout
            .pages()
            .map(|page| usize::try_from(page.chunk.physical_metric.utf16).unwrap())
            .sum::<usize>(),
        text.encode_utf16().count()
    );
}

fn prove_boundary_mapping(
    store: &SourceStore,
    from_root: SourceRootId,
    reset: &ResetCapability,
) -> Option<usize> {
    let mut job = store
        .map_boundary_from(reset.revision, reset.offset, reset.affinity)
        .expect("reset history remains retained");
    loop {
        match job.poll(1) {
            MapStatus::Pending { .. } => {}
            MapStatus::ProvenBoundary(offset) => {
                let proof = job.into_proof().expect("completed boundary proof");
                assert_eq!(proof.from().root, from_root);
                assert_eq!(proof.to(), store.descriptor());
                assert!(matches!(
                    proof.mapping(),
                    ProvenSourceMapping::Boundary {
                        from,
                        to,
                        affinity,
                    } if *from == reset.offset && *to == offset && *affinity == reset.affinity
                ));
                return Some(offset);
            }
            MapStatus::Changed { .. } => return None,
            MapStatus::Failed(error) => panic!("reset lineage failed: {error}"),
            MapStatus::ProvenRange(_) => panic!("boundary job returned a range"),
        }
    }
}

fn prove_range_mapping(
    store: &SourceStore,
    revision: SourceRevision,
    from_root: SourceRootId,
    source: Range<usize>,
) -> Option<Range<usize>> {
    let mut job = store
        .map_range_from(revision, source.clone())
        .expect("page history remains retained");
    loop {
        match job.poll(1) {
            MapStatus::Pending { .. } => {}
            MapStatus::ProvenRange(mapped) => {
                let proof = job.into_proof().expect("completed range proof");
                assert_eq!(proof.from().root, from_root);
                assert_eq!(proof.to(), store.descriptor());
                assert!(matches!(
                    proof.mapping(),
                    ProvenSourceMapping::Range { from, to }
                        if *from == source && *to == mapped
                ));
                return Some(mapped);
            }
            MapStatus::Changed { .. } => return None,
            MapStatus::Failed(error) => panic!("page lineage failed: {error}"),
            MapStatus::ProvenBoundary(_) => panic!("range job returned a boundary"),
        }
    }
}

fn same_page(old: &ProjectionPage, new: &ProjectionPage) -> bool {
    old.chunk.physical_metric == new.chunk.physical_metric
        && old.chunk.logical_contribution == new.chunk.logical_contribution
}

fn measure_reuse(
    store: &SourceStore,
    old: &ProjectionLayout,
    new: &ProjectionLayout,
    edit: &AcceptedEdit,
) -> ReuseReceipt {
    let new_pages = new.pages().collect::<Vec<_>>();
    let mut reused_ranges = BTreeSet::new();
    let mut receipt = ReuseReceipt {
        old_pages: old.page_count(),
        new_pages: new.page_count(),
        ..ReuseReceipt::default()
    };

    for old_page in old.pages() {
        let is_suffix = old_page.source.start >= edit.record.edited_old.end;
        let Some(mapped) =
            prove_range_mapping(store, old.revision, old.root, old_page.source.clone())
        else {
            continue;
        };
        receipt.source_mapped_old_pages += 1;
        receipt.source_mapped_suffix_pages += usize::from(is_suffix);
        if let Some(new_page) = new_pages
            .iter()
            .find(|new_page| new_page.source == mapped && same_page(old_page, new_page))
        {
            receipt.exact_reusable_pages += 1;
            receipt.exact_reusable_suffix_pages += usize::from(is_suffix);
            reused_ranges.insert((new_page.source.start, new_page.source.end));
        }
    }

    receipt.changed_new_pages = receipt.new_pages - receipt.exact_reusable_pages;
    for group in &new.groups {
        let changed_pages = group
            .pages
            .iter()
            .filter(|page| !reused_ranges.contains(&(page.source.start, page.source.end)))
            .count();
        if changed_pages != 0 {
            receipt.changed_new_groups += 1;
            receipt.maximum_changed_group_pages =
                receipt.maximum_changed_group_pages.max(changed_pages);
        }
    }
    receipt
}

fn update_layout(
    store: &SourceStore,
    old: &ProjectionLayout,
    edit: &AcceptedEdit,
    default_affinity: BoundaryAffinity,
) -> (ProjectionLayout, TransitionReceipt) {
    assert_eq!(edit.transition.base_revision, old.revision);
    assert_eq!(edit.transition.base_root, old.root);
    assert_eq!(edit.transition.target_revision, store.revision());
    assert_eq!(edit.transition.result_root, store.root_id());

    let snapshot = store.query_snapshot();
    let text = snapshot.materialize_for_testing();
    let mut reset_receipt = ResetReceipt::default();
    let mut resets = Vec::new();
    for reset in &old.resets {
        if let Some(offset) = prove_boundary_mapping(store, old.root, reset) {
            reset_receipt.mapped += 1;
            resets.push(ResetCapability {
                id: reset.id,
                revision: snapshot.revision(),
                offset,
                affinity: reset.affinity,
            });
        } else {
            reset_receipt.changed += 1;
        }
    }
    let mut next_reset_id = old.next_reset_id;
    rebalance_resets(
        &text,
        snapshot.revision(),
        &mut resets,
        &mut next_reset_id,
        default_affinity,
        &mut reset_receipt,
    );
    let new = build_layout(store, resets, next_reset_id);
    let reuse = measure_reuse(store, old, &new, edit);
    let receipt = TransitionReceipt {
        resets: reset_receipt,
        reuse,
        maximum_group_bytes: new.maximum_group_bytes(),
        maximum_group_pages: new.maximum_group_pages(),
    };
    (new, receipt)
}

#[allow(clippy::cast_precision_loss)] // Diagnostic ratio; tests retain exact page-count assertions.
fn suffix_ratio(receipt: ReuseReceipt) -> f64 {
    if receipt.source_mapped_suffix_pages == 0 {
        return 1.0;
    }
    receipt.exact_reusable_suffix_pages as f64 / receipt.source_mapped_suffix_pages as f64
}

#[test]
fn repeated_prefix_edits_replenish_resets_without_suffix_cascade() {
    let initial = "\0".repeat(64 * 1024);
    let insertion = "\0".repeat(1024);
    let mut store = SourceStore::new(&initial, 64);
    let mut layout = initial_layout(&store, BoundaryAffinity::After);
    let initial_resets = layout.resets.len();
    let mut maximum_changed_groups = 0;
    let mut maximum_changed_pages = 0;
    let mut minimum_suffix_ratio = 1.0_f64;
    let mut minted_resets = 0;

    for _ in 0..24 {
        let edit = store
            .apply_edit(store.revision(), 0..0, &insertion)
            .expect("scalar-aligned prefix insertion");
        let (next, receipt) = update_layout(&store, &layout, &edit, BoundaryAffinity::After);
        maximum_changed_groups = maximum_changed_groups.max(receipt.reuse.changed_new_groups);
        maximum_changed_pages = maximum_changed_pages.max(receipt.reuse.changed_new_pages);
        minimum_suffix_ratio = minimum_suffix_ratio.min(suffix_ratio(receipt.reuse));
        minted_resets += receipt.resets.minted;
        layout = next;
    }

    eprintln!(
        "prefix_edits revisions=24 initial_resets={initial_resets} final_resets={} \
         minted_resets={minted_resets} max_changed_groups={maximum_changed_groups} \
         max_changed_pages={maximum_changed_pages} min_suffix_ratio={minimum_suffix_ratio:.3} \
         max_group_bytes={} max_group_pages={}",
        layout.resets.len(),
        layout.maximum_group_bytes(),
        layout.maximum_group_pages(),
    );
    assert!(layout.resets.len() > initial_resets);
    assert!(minted_resets > 0);
    assert!(maximum_changed_groups <= 2);
    assert!(maximum_changed_pages <= 20);
    // This ratio is document-size dependent; the invariant is the absolute
    // two-group/20-page churn bound. Even this deliberately short fixture
    // retains more than three quarters of its source-mapped suffix pages.
    assert!(minimum_suffix_ratio > 0.75);
    assert!(layout.maximum_group_bytes() <= MAX_GROUP_BYTES);
}

fn insertion_at_reset_case(affinity: BoundaryAffinity) -> (TransitionReceipt, usize, usize) {
    let initial = "\0".repeat(64 * 1024);
    let insertion = "\0".repeat(1024);
    let mut store = SourceStore::new(&initial, 8);
    let layout = initial_layout(&store, affinity);
    let reset = layout.resets[layout.resets.len() / 2].clone();
    let edit = store
        .apply_edit(store.revision(), reset.offset..reset.offset, &insertion)
        .expect("insertion exactly at reset");
    let (next, receipt) = update_layout(&store, &layout, &edit, affinity);
    let mapped = next
        .resets
        .iter()
        .find(|candidate| candidate.id == reset.id)
        .expect("the exact reset maps under either affinity")
        .offset;
    let expected = match affinity {
        BoundaryAffinity::Before => reset.offset,
        BoundaryAffinity::After => reset.offset + insertion.len(),
    };
    assert_eq!(mapped, expected);
    assert!(receipt.reuse.changed_new_groups <= 1);
    assert!(receipt.reuse.changed_new_pages <= 10);
    (receipt, reset.offset, mapped)
}

#[test]
fn insertion_exactly_at_reset_is_local_under_both_affinities() {
    let (before, before_old, before_new) = insertion_at_reset_case(BoundaryAffinity::Before);
    let (after, after_old, after_new) = insertion_at_reset_case(BoundaryAffinity::After);
    eprintln!(
        "reset_insert before={before_old}->{before_new} before_changed_pages={} \
         before_suffix_ratio={:.3} after={after_old}->{after_new} after_changed_pages={} \
         after_suffix_ratio={:.3}",
        before.reuse.changed_new_pages,
        suffix_ratio(before.reuse),
        after.reuse.changed_new_pages,
        suffix_ratio(after.reuse),
    );
    assert_eq!(before_new, before_old);
    assert!(after_new > after_old);
    assert!(suffix_ratio(before.reuse) > 0.70);
    assert!(suffix_ratio(after.reuse) > 0.95);
}

#[test]
fn deletion_across_resets_drops_only_intersected_anchors() {
    let initial = "\0".repeat(96 * 1024);
    let mut store = SourceStore::new(&initial, 8);
    let layout = initial_layout(&store, BoundaryAffinity::After);
    let deleted = layout.resets[2].offset + 1000..layout.resets[6].offset + 2000;
    let edit = store
        .apply_edit(store.revision(), deleted.clone(), "")
        .expect("deletion across reset anchors");
    let (next, receipt) = update_layout(&store, &layout, &edit, BoundaryAffinity::After);
    eprintln!(
        "cross_reset_delete bytes={} mapped_resets={} changed_resets={} underflow_removed={} \
         old_pages={} new_pages={} changed_groups={} changed_pages={} suffix={}/{} \
         max_group_bytes={} max_group_pages={}",
        deleted.len(),
        receipt.resets.mapped,
        receipt.resets.changed,
        receipt.resets.removed_underflow,
        receipt.reuse.old_pages,
        receipt.reuse.new_pages,
        receipt.reuse.changed_new_groups,
        receipt.reuse.changed_new_pages,
        receipt.reuse.exact_reusable_suffix_pages,
        receipt.reuse.source_mapped_suffix_pages,
        receipt.maximum_group_bytes,
        receipt.maximum_group_pages,
    );
    assert!(receipt.resets.changed >= 3);
    assert!(receipt.reuse.changed_new_groups <= 2);
    assert!(receipt.reuse.changed_new_pages <= 20);
    assert!(suffix_ratio(receipt.reuse) > 0.80);
    assert!(next.maximum_group_bytes() <= MAX_GROUP_BYTES);
}

#[test]
fn underflow_merge_and_later_growth_stay_local() {
    let initial = "\0".repeat(64 * 1024);
    let mut store = SourceStore::new(&initial, 8);
    let layout = initial_layout(&store, BoundaryAffinity::After);

    let delete = store
        .apply_edit(store.revision(), 0..5 * 1024, "")
        .expect("prefix deletion underflows the first reset group");
    let (after_delete, delete_receipt) =
        update_layout(&store, &layout, &delete, BoundaryAffinity::After);
    assert_eq!(delete_receipt.resets.removed_underflow, 1);
    assert!(delete_receipt.reuse.changed_new_groups <= 1);
    assert!(delete_receipt.reuse.changed_new_pages <= 12);

    let insertion = "\0".repeat(20 * 1024);
    let insert = store
        .apply_edit(store.revision(), 0..0, &insertion)
        .expect("prefix growth replenishes the merged group");
    let (after_insert, insert_receipt) =
        update_layout(&store, &after_delete, &insert, BoundaryAffinity::After);
    eprintln!(
        "underflow_growth removed={} delete_changed_pages={} minted={} \
         insert_changed_groups={} insert_changed_pages={} max_group_bytes={}",
        delete_receipt.resets.removed_underflow,
        delete_receipt.reuse.changed_new_pages,
        insert_receipt.resets.minted,
        insert_receipt.reuse.changed_new_groups,
        insert_receipt.reuse.changed_new_pages,
        after_insert.maximum_group_bytes(),
    );
    assert!(insert_receipt.resets.minted >= 2);
    assert!(insert_receipt.reuse.changed_new_groups <= 4);
    assert!(insert_receipt.reuse.changed_new_pages <= 40);
    assert!(after_insert.maximum_group_bytes() <= MAX_GROUP_BYTES);
}

#[test]
fn large_dense_atomic_insertion_replenishes_local_resets() {
    let initial = "\0\t\r\n".repeat(20 * 1024);
    let insertion = "\0\t\r\n".repeat(10 * 1024);
    let mut store = SourceStore::new(&initial, 8);
    let layout = initial_layout(&store, BoundaryAffinity::After);
    let group = &layout.groups[2];
    let insertion_offset = group.source.start + 1024;
    assert!(is_legal_projection_boundary(&initial, insertion_offset));
    let edit = store
        .apply_edit(
            store.revision(),
            insertion_offset..insertion_offset,
            &insertion,
        )
        .expect("large dense atomic insertion");
    let (next, receipt) = update_layout(&store, &layout, &edit, BoundaryAffinity::After);
    let current = store.query_snapshot().materialize_for_testing();
    eprintln!(
        "dense_atomic_growth inserted_bytes={} minted_resets={} changed_groups={} changed_pages={} \
         suffix_ratio={:.3} max_group_bytes={} max_group_pages={}",
        insertion.len(),
        receipt.resets.minted,
        receipt.reuse.changed_new_groups,
        receipt.reuse.changed_new_pages,
        suffix_ratio(receipt.reuse),
        receipt.maximum_group_bytes,
        receipt.maximum_group_pages,
    );
    assert!(receipt.resets.minted >= 4);
    assert!(receipt.reuse.changed_new_groups <= receipt.resets.minted + 2);
    assert!(receipt.reuse.maximum_changed_group_pages <= 20);
    assert!(suffix_ratio(receipt.reuse) > 0.70);
    assert!(
        next.resets
            .iter()
            .all(|reset| is_legal_projection_boundary(&current, reset.offset))
    );
}

#[test]
fn mapped_reset_that_becomes_a_crlf_interior_is_discarded_locally() {
    let initial = "\0\t\n".repeat(32 * 1024);
    let mut store = SourceStore::new(&initial, 8);
    let layout = initial_layout(&store, BoundaryAffinity::After);
    let reset = layout
        .resets
        .iter()
        .find(|reset| {
            initial.as_bytes()[reset.offset - 1] == b'\t'
                && initial.as_bytes()[reset.offset] == b'\n'
        })
        .expect("fixture supplies a legal tab/LF reset")
        .clone();
    let edit = store
        .apply_edit(store.revision(), reset.offset - 1..reset.offset, "\r")
        .expect("tab-to-CR replacement");
    let (next, receipt) = update_layout(&store, &layout, &edit, BoundaryAffinity::After);
    let current = store.query_snapshot().materialize_for_testing();
    assert!(!is_legal_projection_boundary(&current, reset.offset));
    assert!(receipt.resets.dropped_illegal >= 1);
    assert!(next.resets.iter().all(|candidate| candidate.id != reset.id));
    assert!(receipt.reuse.changed_new_groups <= 2);
    assert!(receipt.reuse.changed_new_pages <= 20);
}

#[test]
fn unicode_byte_and_utf16_metrics_survive_source_mapped_resets() {
    let initial = "😀\0é\0".repeat(10 * 1024);
    let insertion = "界\0🦀\0".repeat(113);
    let mut store = SourceStore::new(&initial, 8);
    let layout = initial_layout(&store, BoundaryAffinity::After);
    assert_ne!(layout.source_bytes, layout.source_utf16);
    let edit = store
        .apply_edit(store.revision(), 0..0, &insertion)
        .expect("Unicode prefix insertion");
    let (next, receipt) = update_layout(&store, &layout, &edit, BoundaryAffinity::After);
    let current = store.query_snapshot().materialize_for_testing();
    eprintln!(
        "unicode_metrics bytes={} utf16={} changed_groups={} changed_pages={} suffix_ratio={:.3}",
        next.source_bytes,
        next.source_utf16,
        receipt.reuse.changed_new_groups,
        receipt.reuse.changed_new_pages,
        suffix_ratio(receipt.reuse),
    );
    assert_eq!(next.source_bytes, current.len());
    assert_eq!(next.source_utf16, current.encode_utf16().count());
    assert_ne!(next.source_bytes, next.source_utf16);
    assert!(receipt.reuse.changed_new_groups <= 1);
    assert!(receipt.reuse.changed_new_pages <= 12);
    assert!(suffix_ratio(receipt.reuse) > 0.80);
    assert!(
        next.pages()
            .all(|page| match &page.chunk.logical_contribution {
                LogicalContribution::Program(program) => {
                    program.physical_metric() == page.chunk.physical_metric
                }
                _ => true,
            })
    );
}
