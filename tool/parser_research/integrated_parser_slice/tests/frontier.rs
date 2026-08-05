use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Weak};

use flark_integrated_parser_slice::frontier::{
    BaseLeafKind, CursorStep, FrontierError, LeafId, LeafMetadataOverlay, LeafOutputFrontier,
    LexerStatus, LexicalEventKind, LogicalOrigin, RetainedBytes, SegmentDescriptor, SegmentedLeaf,
    SegmentedLeafBuilder, SharedLexer, TablePipeSummary, VirtualReason, MAX_LEXER_POLL_WORK,
};
use flark_integrated_parser_slice::source::{PersistentSource, SourceCapture, MAX_PIECE_BYTES};

fn drain_leaf(leaf: &SegmentedLeaf) -> (Vec<u8>, Vec<LogicalOrigin>, usize) {
    let mut cursor = leaf.cursor();
    let mut bytes = Vec::new();
    let mut origins = Vec::new();
    loop {
        match cursor.step() {
            CursorStep::Byte(byte) => {
                bytes.push(byte.byte);
                origins.push(byte.origin);
            }
            CursorStep::Progress => {}
            CursorStep::Done => return (bytes, origins, cursor.metrics().operations),
        }
    }
}

fn lex_to_ready(leaf: &SegmentedLeaf, fuel: usize) -> SharedLexer {
    let mut lexer = SharedLexer::new(leaf);
    loop {
        let receipt = lexer.poll(fuel);
        assert!(receipt.work <= fuel);
        assert!(receipt.work <= MAX_LEXER_POLL_WORK);
        if receipt.status == LexerStatus::Ready {
            return lexer;
        }
    }
}

fn dense_event_receipt(input_bytes: usize) -> (usize, usize, RetainedBytes, RetainedBytes) {
    let pattern = b"[]|";
    let mut text = String::with_capacity(input_bytes);
    for index in 0..input_bytes {
        text.push(char::from(pattern[index % pattern.len()]));
    }
    let source = Arc::new(PersistentSource::from_text(&text));
    let mut builder = SegmentedLeafBuilder::new(source);
    builder.push_source(0..text.len()).unwrap();
    let leaf = builder.finish();
    let descriptors = leaf.retained_descriptor_bytes();
    let lexer = lex_to_ready(&leaf, MAX_LEXER_POLL_WORK);
    let consumers = lexer.consumers().unwrap();
    let view = consumers.inline.view();
    (
        view.event_count(),
        view.page_count(),
        view.retained_event_bytes(),
        descriptors,
    )
}

fn fixed_thousandths(total: usize, count: usize) -> (usize, usize) {
    let scaled = total.saturating_mul(1000) / count;
    (scaled / 1000, scaled % 1000)
}

#[test]
fn quote_and_list_prefixes_map_to_physical_source_without_entering_logical_text() {
    let text = "> - first\n>   second";
    let source = Arc::new(PersistentSource::from_text(text));
    let second = text.find("second").unwrap();
    let mut builder = SegmentedLeafBuilder::new(source.clone());
    builder.push_source(4..9).unwrap();
    builder.push_virtual_newline(9).unwrap();
    builder.push_source(second..text.len()).unwrap();
    let leaf = builder.finish();

    let (bytes, origins, _) = drain_leaf(&leaf);
    assert_eq!(bytes, b"first\nsecond");
    assert_eq!(
        origins[0],
        LogicalOrigin::Source(source.anchor_at(4).unwrap())
    );
    assert_eq!(
        origins[5],
        LogicalOrigin::Virtual {
            attachment: match leaf.descriptors().nth(1).unwrap() {
                SegmentDescriptor::Virtual { attachment, .. } => attachment,
                SegmentDescriptor::Source(_) => {
                    panic!("line join must be virtual")
                }
            },
            reason: VirtualReason::ContainerLineJoin,
        }
    );
    assert_eq!(
        origins[6],
        LogicalOrigin::Source(source.anchor_at(second).unwrap())
    );
    assert!(!bytes.windows(2).any(|window| window == b"> "));
    assert!(!bytes.windows(2).any(|window| window == b"- "));
}

#[test]
fn sparse_logical_spans_seek_over_large_excluded_source_without_scanning_it() {
    const GAP: usize = 1024 * 1024;
    let text = format!("a{}b", "x".repeat(GAP));
    let source = Arc::new(PersistentSource::from_text(&text));
    let mut builder = SegmentedLeafBuilder::new(source);
    builder.push_source(0..1).unwrap();
    builder.push_virtual_newline(1).unwrap();
    builder.push_source(text.len() - 1..text.len()).unwrap();
    let leaf = builder.finish();

    let mut cursor = leaf.cursor();
    let mut logical = Vec::new();
    loop {
        match cursor.step() {
            CursorStep::Byte(byte) => logical.push(byte.byte),
            CursorStep::Progress => {}
            CursorStep::Done => break,
        }
    }
    assert_eq!(logical, b"a\nb");
    let metrics = cursor.metrics();
    assert_eq!(metrics.source_seek_operations, 1);
    assert!(metrics.source_seek_index_nodes > 0);
    assert_eq!(metrics.excluded_source_bytes, 1);
    assert_eq!(metrics.skipped_source_bytes, GAP - 1);
    assert!(metrics.operations < 16, "{metrics:?}");
}

#[test]
fn partial_tab_indentation_is_virtual_and_has_no_invented_source_anchor() {
    let text = ">\titem";
    let source = Arc::new(PersistentSource::from_text(text));
    let mut builder = SegmentedLeafBuilder::new(source.clone());
    builder.push_virtual_tab_spaces(1, 3).unwrap();
    builder.push_source(2..text.len()).unwrap();
    let leaf = builder.finish();

    let (bytes, origins, _) = drain_leaf(&leaf);
    assert_eq!(bytes, b"   item");
    for origin in &origins[..3] {
        assert!(matches!(
            origin,
            LogicalOrigin::Virtual {
                reason: VirtualReason::TabExpansion,
                ..
            }
        ));
    }
    assert_eq!(
        origins[3],
        LogicalOrigin::Source(source.anchor_at(2).unwrap())
    );
}

#[test]
fn sealed_leaf_drops_the_originating_source_root_but_keeps_exact_bounded_bytes() {
    let text = format!("outside\n{}\nafter", "inside".repeat(2_000));
    let start = text.find("inside").unwrap();
    let end = text.rfind("\nafter").unwrap();
    let source = Arc::new(PersistentSource::from_text(&text));
    let weak: Weak<PersistentSource> = Arc::downgrade(&source);
    let first = source.anchor_at(start).unwrap();
    let mut builder = SegmentedLeafBuilder::new(source.clone());
    builder.push_source(start..end).unwrap();
    let leaf = builder.finish();
    drop(source);

    assert!(
        weak.upgrade().is_none(),
        "sealed leaves must not retain Arc<PersistentSource>"
    );
    assert_eq!(drain_leaf(&leaf).0, text.as_bytes()[start..end]);
    assert_eq!(
        drain_leaf(&leaf).1.first(),
        Some(&LogicalOrigin::Source(first))
    );
    let retained = leaf.retained_source_metrics();
    assert_eq!(retained.referenced_piece_bytes, end - start);
    assert!(retained.unreferenced_retained_bytes <= 2 * MAX_PIECE_BYTES);
}

#[test]
fn certified_scan_capture_removes_descriptor_seeks_and_fragment_reseek() {
    let text = "alpha\nβeta";
    let source = Arc::new(PersistentSource::from_text(text));
    let mut scan = source.cursor();
    let start = scan.certified_boundary().unwrap();
    let mut capture = SourceCapture::new(start);
    for _ in 0.."alpha".len() {
        scan.next_captured(&mut capture).unwrap().unwrap();
    }
    let end_first = scan.certified_boundary().unwrap();
    scan.next_captured(&mut capture).unwrap().unwrap();
    let start_second = scan.certified_boundary().unwrap();
    while scan.next_captured(&mut capture).unwrap().is_some() {}
    let end = scan.certified_boundary().unwrap();
    let captured = capture.finish(end).unwrap();

    let mut builder = SegmentedLeafBuilder::new(source);
    builder.push_certified_source(start, end_first).unwrap();
    builder.push_certified_virtual_newline(end_first).unwrap();
    builder.push_certified_source(start_second, end).unwrap();
    let leaf = builder.finish_with_capture(captured).unwrap();
    let receipt = leaf.construction_metrics();

    assert_eq!(drain_leaf(&leaf).0, "alpha\nβeta".as_bytes());
    assert!(receipt.used_sequential_capture);
    assert_eq!(receipt.boundary_index_nodes_visited, 0);
    assert_eq!(receipt.boundary_bytes_examined, 0);
    assert_eq!(receipt.fragment_extraction.structural_nodes_allocated, 0);
    assert_eq!(receipt.sequential_capture.bytes_observed, text.len());
    assert_eq!(receipt.sequential_capture.payload_bytes_copied, 0);
}

#[test]
fn exact_rebind_accepts_shifted_anchors_and_rejects_same_length_suffix_edit() {
    let text = "header\nsuffix **body**\n";
    let suffix = text.find("suffix").unwrap();
    let original = PersistentSource::from_text(text);
    let stable_first = original.anchor_at(suffix).unwrap();
    let mut builder = SegmentedLeafBuilder::new(Arc::new(original.clone()));
    builder.push_source(suffix..text.len()).unwrap();
    builder.push_virtual_newline(text.len()).unwrap();
    let leaf = builder.finish();

    let prefix = "inserted\n";
    let shifted_source = original.edit(0..0, prefix).unwrap().source;
    let (rebound, receipt) = leaf
        .rebind_to_current(&shifted_source, suffix + prefix.len())
        .unwrap();
    let SegmentDescriptor::Source(first_span) = rebound.descriptors().next().unwrap() else {
        panic!("first descriptor is physical")
    };
    assert_eq!(first_span.document.start, suffix + prefix.len());
    assert_eq!(first_span.first, stable_first);
    assert_eq!(
        rebound.descriptors().last().unwrap(),
        SegmentDescriptor::Virtual {
            byte: b'\n',
            count: 1,
            attachment: flark_integrated_parser_slice::frontier::VirtualAttachment {
                document_offset: shifted_source.len_bytes(),
                anchor: shifted_source.anchor_at(shifted_source.len_bytes() - 1),
                after_anchor: true,
            },
            reason: VirtualReason::ContainerLineJoin,
        }
    );
    assert_eq!(receipt.payload_bytes_copied, 0);

    let body = shifted_source
        .materialize()
        .find("body")
        .expect("body remains present");
    let changed = shifted_source.edit(body..body + 1, "B").unwrap().source;
    assert!(matches!(
        rebound.rebind_to_current(&changed, suffix + prefix.len()),
        Err(flark_integrated_parser_slice::frontier::LeafRebindError::StableAnchorLayoutChanged)
    ));
}

#[test]
fn mixed_thousand_revision_leaves_retain_current_buffers_not_historical_roots() {
    const REVISIONS: usize = 1_024;
    let block = "x".repeat(MAX_PIECE_BYTES);
    let mut current = PersistentSource::default();
    let mut leaves = Vec::with_capacity(REVISIONS);
    let mut dead_roots = Vec::with_capacity(REVISIONS);

    for revision in 0..REVISIONS {
        let end = current.len_bytes();
        current = current.edit(end..end, &block).unwrap().source;
        if revision > 0 {
            // The immediately preceding append was compacted once by this
            // edit and is now outside the mutable right boundary page.
            let start = (revision - 1) * MAX_PIECE_BYTES;
            let root = Arc::new(current.clone());
            dead_roots.push(Arc::downgrade(&root));
            let mut builder = SegmentedLeafBuilder::new(root.clone());
            builder.push_source(start..start + MAX_PIECE_BYTES).unwrap();
            leaves.push(builder.finish());
            drop(root);
        }
    }
    let last = (REVISIONS - 1) * MAX_PIECE_BYTES;
    let root = Arc::new(current.clone());
    dead_roots.push(Arc::downgrade(&root));
    let mut builder = SegmentedLeafBuilder::new(root.clone());
    builder.push_source(last..current.len_bytes()).unwrap();
    leaves.push(builder.finish());
    drop(root);

    assert!(dead_roots.iter().all(|root| root.upgrade().is_none()));
    assert_eq!(leaves.len(), REVISIONS);
    let leaf_buffers = leaves
        .iter()
        .flat_map(SegmentedLeaf::retained_source_buffer_ids)
        .collect::<BTreeSet<_>>();
    let current_buffers = current
        .retained_buffer_ids()
        .into_iter()
        .collect::<BTreeSet<_>>();
    assert_eq!(leaf_buffers, current_buffers);
    let retained_bytes = leaves
        .iter()
        .map(|leaf| leaf.retained_source_metrics().retained_buffer_bytes)
        .sum::<usize>();
    assert_eq!(retained_bytes, current.len_bytes());
    let unique_leaf_buffers = leaves
        .iter()
        .flat_map(SegmentedLeaf::retained_source_buffer_allocations)
        .map(|buffer| (buffer.id, buffer.bytes))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(unique_leaf_buffers.len(), current_buffers.len());
    assert_eq!(
        unique_leaf_buffers.values().sum::<usize>(),
        current.buffer_retention().retained_buffer_bytes
    );
}

#[test]
fn virtual_attachments_cannot_split_a_utf8_scalar() {
    let source = Arc::new(PersistentSource::from_text("é"));
    let mut builder = SegmentedLeafBuilder::new(source);
    assert_eq!(
        builder.push_virtual_newline(1),
        Err(FrontierError::SourceBoundarySplitsScalar(1))
    );
}

#[test]
fn separate_empty_inputs_do_not_false_match_as_one_lexical_root() {
    let source = Arc::new(PersistentSource::from_text(""));
    let left_leaf = SegmentedLeafBuilder::new(source.clone()).finish();
    let right_leaf = SegmentedLeafBuilder::new(source).finish();
    let left = lex_to_ready(&left_leaf, 1).consumers().unwrap();
    let right = lex_to_ready(&right_leaf, 1).consumers().unwrap();

    assert!(left.table.view().shares_root_with(left.inline.view()));
    assert!(!left.table.view().shares_root_with(right.inline.view()));
    assert_ne!(left_leaf.identity(), right_leaf.identity());
}

#[test]
fn table_and_inline_consumers_share_escape_code_and_pipe_event_pages() {
    // GFM tables require the pipe inside `c\|d` to be escaped, even though it
    // is inside code. The unescaped pipe in `e|f` remains a table delimiter.
    let text = r"`c\|d` | `e|f`";
    let source = Arc::new(PersistentSource::from_text(text));
    let mut builder = SegmentedLeafBuilder::new(source);
    builder.push_source(0..text.len()).unwrap();
    let leaf = builder.finish();
    let lexer = lex_to_ready(&leaf, 7);
    let consumers = lexer.consumers().unwrap();

    assert!(consumers
        .table
        .view()
        .shares_root_with(consumers.inline.view()));
    let table = consumers.table.classify();
    assert_eq!(table.logical_offsets, vec![7, 11]);
    assert_eq!(table.receipt.source_bytes_examined, 0);

    let events = consumers
        .inline
        .events()
        .map(|event| (event.start.offset, event.kind))
        .collect::<Vec<_>>();
    assert!(events.contains(&(2, LexicalEventKind::BackslashEscape { escaped: b'|' })));
    assert!(!events.contains(&(3, LexicalEventKind::TablePipe)));
    assert!(events.contains(&(7, LexicalEventKind::TablePipe)));
    assert!(events.contains(&(11, LexicalEventKind::TablePipe)));
    assert_eq!(
        consumers.inline.audit().source_bytes_examined,
        0,
        "candidate enumeration itself does not inspect logical text"
    );
    assert_eq!(
        table.receipt.lexical_events_examined,
        consumers.inline.audit().lexical_events_examined
    );
}

#[test]
fn giant_emphasis_run_coalesces_across_source_descriptors_under_four_kib_fuel() {
    let text = "*".repeat(1024 * 1024);
    let source = Arc::new(PersistentSource::from_text(&text));
    let mut builder = SegmentedLeafBuilder::new(source);
    for start in (0..text.len()).step_by(4096) {
        builder
            .push_source(start..(start + 4096).min(text.len()))
            .unwrap();
    }

    let leaf = builder.finish();
    let mut lexer = SharedLexer::new(&leaf);
    let mut polls = 0;
    loop {
        let receipt = lexer.poll(MAX_LEXER_POLL_WORK);
        polls += 1;
        assert!(receipt.work <= MAX_LEXER_POLL_WORK);
        if receipt.status == LexerStatus::Ready {
            break;
        }
    }
    assert!(polls > 1);
    assert_eq!(lexer.max_poll_work(), MAX_LEXER_POLL_WORK);
    let events = lexer
        .consumers()
        .unwrap()
        .inline
        .events()
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].kind,
        LexicalEventKind::EmphasisRun {
            marker: b'*',
            len: text.len()
        }
    );
}

#[test]
fn one_million_segment_cursor_is_linear_and_never_searches_by_logical_offset() {
    const SEGMENTS: usize = 1_000_000;
    let source = Arc::new(PersistentSource::from_text(""));
    let mut builder = SegmentedLeafBuilder::new(source);
    for _ in 0..SEGMENTS {
        builder.push_virtual_tab_spaces(0, 1).unwrap();
    }
    let leaf = builder.finish();
    let mut cursor = leaf.cursor();
    let mut bytes = 0;
    loop {
        match cursor.step() {
            CursorStep::Byte(byte) => {
                assert_eq!(byte.byte, b' ');
                bytes += 1;
            }
            CursorStep::Progress => {}
            CursorStep::Done => break,
        }
    }
    let metrics = cursor.metrics();
    assert_eq!(bytes, SEGMENTS);
    assert_eq!(metrics.logical_bytes, SEGMENTS);
    assert_eq!(metrics.descriptor_entries, SEGMENTS);
    assert_eq!(metrics.operations, SEGMENTS * 2);

    let retained = leaf.retained_descriptor_bytes();
    let (whole, fraction) = fixed_thousandths(retained.total(), SEGMENTS);
    eprintln!(
        "descriptor-density input_segments={SEGMENTS} pages={} accounted_bytes={} bytes_per_segment={whole}.{fraction:03} allocations={}",
        leaf.descriptor_page_count(),
        retained.total(),
        retained.allocations,
    );
    assert!(
        retained.total() <= SEGMENTS * 12,
        "the packed descriptor root must stay within the hard density target"
    );
    assert!(leaf.descriptor_page_count() < SEGMENTS / 1000);
    assert_eq!(retained.allocations, leaf.descriptor_page_count() * 4 - 1);
}

#[test]
fn ten_mib_quote_list_shape_keeps_alternating_descriptors_compact_and_linear() {
    const SOURCE_BYTES: usize = 10 * 1024 * 1024;
    const LINE_BYTES: usize = 64;
    const PREFIX_BYTES: usize = 4;
    const LINES: usize = SOURCE_BYTES / LINE_BYTES;
    let line = format!("> - {}\n", "x".repeat(LINE_BYTES - PREFIX_BYTES - 1));
    assert_eq!(line.len(), LINE_BYTES);
    let text = line.repeat(LINES);
    assert_eq!(text.len(), SOURCE_BYTES);

    let source = Arc::new(PersistentSource::from_text(&text));
    let mut builder = SegmentedLeafBuilder::new(source);
    for line_start in (0..SOURCE_BYTES).step_by(LINE_BYTES) {
        let newline = line_start + LINE_BYTES - 1;
        builder
            .push_source(line_start + PREFIX_BYTES..newline)
            .unwrap();
        builder.push_virtual_newline(newline).unwrap();
    }
    let leaf = builder.finish();
    let expected_segments = LINES * 2;
    assert_eq!(leaf.descriptor_count(), expected_segments);

    let mut cursor = leaf.cursor();
    while cursor.step() != CursorStep::Done {}
    let metrics = cursor.metrics();
    assert_eq!(metrics.descriptor_entries, expected_segments);
    assert_eq!(metrics.logical_bytes, LINES * (LINE_BYTES - PREFIX_BYTES));

    let retained = leaf.retained_descriptor_bytes();
    let (whole, fraction) = fixed_thousandths(retained.total(), expected_segments);
    eprintln!(
        "quote-list-density source_bytes={SOURCE_BYTES} segments={expected_segments} pages={} accounted_bytes={} bytes_per_segment={whole}.{fraction:03} cursor_operations={} allocations={}",
        leaf.descriptor_page_count(),
        retained.total(),
        metrics.operations,
        retained.allocations,
    );
    assert!(retained.total() <= expected_segments * 12);
    assert_eq!(leaf.descriptors().count(), expected_segments);
}

#[test]
fn edited_suffix_descriptor_recovers_unchanged_stable_anchors_from_source_root() {
    let text = "header\nsuffix **body**\n";
    let suffix = text.find("suffix").unwrap();
    let original = PersistentSource::from_text(text);
    let first_anchor = original.anchor_at(suffix).unwrap();
    let last_anchor = original.anchor_at(text.len() - 1).unwrap();
    let insertion = "inserted\n";
    let edit = original.edit(0..0, insertion).unwrap();
    let shifted = edit.provenance.map_old_byte(suffix).unwrap();

    let source = Arc::new(edit.source);
    let mut builder = SegmentedLeafBuilder::new(source.clone());
    builder.push_source(shifted..source.len_bytes()).unwrap();
    let leaf = builder.finish();
    let descriptor = leaf.descriptors().next().unwrap();
    let SegmentDescriptor::Source(span) = descriptor else {
        panic!("suffix must remain a physical descriptor");
    };
    assert_eq!(span.first, first_anchor);
    assert_eq!(span.last, last_anchor);

    let mut cursor = leaf.cursor();
    assert_eq!(cursor.step(), CursorStep::Progress);
    let CursorStep::Byte(first) = cursor.step() else {
        panic!("suffix cursor must yield its first physical byte");
    };
    assert_eq!(first.origin, LogicalOrigin::Source(first_anchor));

    let retained = leaf.retained_descriptor_bytes();
    eprintln!(
        "edited-suffix descriptor_bytes={} allocations={} stable_first_buffer={} stable_first_offset={}",
        retained.total(),
        retained.allocations,
        first_anchor.buffer_id.0,
        first_anchor.offset,
    );
    assert!(retained.total() <= 160);
}

#[test]
fn one_mib_event_per_byte_input_has_compact_linear_retained_shape() {
    const SMALL: usize = 256 * 1024;
    const LARGE: usize = 1024 * 1024;
    let (small_events, _, small, _) = dense_event_receipt(SMALL);
    let (events, pages, retained, descriptors) = dense_event_receipt(LARGE);
    let (whole, fraction) = fixed_thousandths(retained.total(), events);

    eprintln!(
        "dense-lexical input_bytes={LARGE} events={events} pages={pages} encoded_payload={} accounted_bytes={} bytes_per_event={whole}.{fraction:03} allocations={} descriptor_bytes={}",
        retained.payload,
        retained.total(),
        retained.allocations,
        descriptors.total(),
    );

    assert_eq!(small_events, SMALL);
    assert_eq!(events, LARGE);
    assert_eq!(
        retained.payload, LARGE,
        "dense one-byte events encode to one byte"
    );
    assert!(
        retained.total() < LARGE * 4,
        "event pages must not recreate a struct-per-byte memory explosion"
    );
    assert!(
        retained.total() >= small.total() * 3 && retained.total() <= small.total() * 5,
        "accounted retention should grow linearly from 256 KiB to 1 MiB"
    );
    assert_eq!(descriptors.allocations, 3);
}

#[test]
fn setext_and_table_promotions_are_keyed_overlays_over_an_immutable_prefix() {
    let text = "heading|value";
    let source = Arc::new(PersistentSource::from_text(text));
    let id = LeafId(41);
    let mut frontier = LeafOutputFrontier::default();
    frontier
        .begin_leaf(id, BaseLeafKind::Paragraph, source)
        .unwrap();
    frontier
        .open_input()
        .unwrap()
        .push_source(0..text.len())
        .unwrap();
    let sealed = frontier.seal_open().unwrap();

    frontier.promote_setext(id, 2).unwrap();
    let after_setext = frontier.sealed_leaf(id).unwrap();
    assert!(Arc::ptr_eq(&sealed, &after_setext));
    assert_eq!(
        frontier.overlay(id),
        Some(&LeafMetadataOverlay::Setext { level: 2 })
    );

    let pipes = TablePipeSummary {
        logical_offsets: vec![7],
        ..TablePipeSummary::default()
    };
    frontier.promote_table(id, &pipes).unwrap();
    let after_table = frontier.sealed_leaf(id).unwrap();
    assert!(Arc::ptr_eq(&sealed, &after_table));
    assert!(matches!(
        frontier.overlay(id),
        Some(LeafMetadataOverlay::Table {
            columns: 2,
            pipe_offsets,
        }) if pipe_offsets.as_ref() == [7]
    ));
    assert_eq!(sealed.base_kind, BaseLeafKind::Paragraph);

    assert_eq!(
        frontier.begin_leaf(
            id,
            BaseLeafKind::Paragraph,
            Arc::new(PersistentSource::default())
        ),
        Err(FrontierError::DuplicateLeaf(id))
    );
}
