use super::{
    splice_m11_recursive_green_coverage_atomic, splice_m11_recursive_green_structural_atomic,
    M11RecursiveGreenBuild, M11RecursiveGreenBuildStatus, M11RecursiveGreenCachedRowEditCapability,
    M11RecursiveGreenCachedRowEditable, M11RecursiveGreenCloseFacts, M11RecursiveGreenClosedChild,
    M11RecursiveGreenCoveragePart, M11RecursiveGreenError, M11RecursiveGreenEvent,
    M11RecursiveGreenFactTag, M11RecursiveGreenFrameId, M11RecursiveGreenKind,
    M11RecursiveGreenLogicalAction, M11RecursiveGreenLogicalAtom, M11RecursiveGreenLogicalPosition,
    M11RecursiveGreenLogicalRange, M11RecursiveGreenPoint, M11RecursiveGreenRoot,
    M11RecursiveGreenSourceMetric, M11RecursiveGreenStructuralBoundary,
    M11RecursiveGreenTerminalFragmentBarrierStatus, M11RecursiveGreenTerminalFragmentCursorStatus,
    M11RecursiveGreenTerminalFragmentDisposition, M11RecursiveGreenTerminalFragmentRewrite,
    M11RecursiveGreenTerminalFragmentRewritePoll,
};
use crate::{
    DocumentRuntime, DocumentRuntimeConfig, SourceBoundaryAffinity, ARENA_PAGE_BYTES,
    SOURCE_CURSOR_WINDOW_BYTES,
};

use super::codec::{
    decode_leaf, decode_packed_event, encode_leaf_header, encode_packed_event, packed_event_len,
    packed_event_summary, LogicalAtom, PackedGreenEvent, RecursiveGreenSummary,
    GREEN_EVENTS_PER_PAGE_MAX,
};
use crate::measured_sequence::{SequenceInspectionReceipt, SequenceSpecInspection};

fn frame(value: u64) -> M11RecursiveGreenFrameId {
    M11RecursiveGreenFrameId::new(value).expect("nonzero frame")
}

fn kind(value: u16) -> M11RecursiveGreenKind {
    M11RecursiveGreenKind::new(value).expect("nonzero kind")
}

fn metric(bytes: u64, utf16: u64) -> M11RecursiveGreenSourceMetric {
    M11RecursiveGreenSourceMetric::new(bytes, utf16).expect("valid metric")
}

#[test]
fn dense_common_events_use_minimal_varints_and_reject_noncanonical_forms() {
    let enter = PackedGreenEvent::Enter {
        frame: frame(1),
        kind: kind(1),
    };
    let coverage = PackedGreenEvent::Coverage {
        physical: metric(2, 2),
        owner_depth: 0,
        part: M11RecursiveGreenCoveragePart::Content,
        atom: LogicalAtom::Identity,
    };
    let exit = PackedGreenEvent::Exit {
        frame: frame(1),
        final_kind: kind(1),
        close: None,
        last_line_blank: false,
        child: M11RecursiveGreenClosedChild::default(),
    };
    assert_eq!(packed_event_len(enter), 3);
    assert_eq!(packed_event_len(coverage), 6);
    assert_eq!(packed_event_len(exit), 6);

    let mut page = [0_u8; ARENA_PAGE_BYTES];
    let mut cursor = 0;
    for event in [enter, coverage, exit] {
        encode_packed_event(event, &mut page, &mut cursor).expect("encode dense event");
    }
    assert_eq!(cursor, 15);

    let mut nonminimal_cursor = 0;
    let nonminimal = [1, 0x81, 0x00, 1];
    assert!(decode_packed_event(&nonminimal, &mut nonminimal_cursor).is_err());

    let mut overflow_cursor = 0;
    let overflow = [
        1, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x02, 1,
    ];
    assert!(decode_packed_event(&overflow, &mut overflow_cursor).is_err());
}

fn offer(
    build: &mut M11RecursiveGreenBuild,
    runtime: &mut DocumentRuntime,
    event: M11RecursiveGreenEvent,
) {
    build.offer_event(event).expect("offer event");
    loop {
        let poll = build.poll(runtime, 1).expect("poll offered event");
        if poll.status() == M11RecursiveGreenBuildStatus::NeedsInput {
            break;
        }
        assert_eq!(poll.status(), M11RecursiveGreenBuildStatus::Pending);
    }
}

fn offer_fast(
    build: &mut M11RecursiveGreenBuild,
    runtime: &mut DocumentRuntime,
    event: M11RecursiveGreenEvent,
) {
    build.offer_event(event).expect("offer event");
    loop {
        let poll = build
            .poll(runtime, super::M11_RECURSIVE_GREEN_MAX_POLL_TRANSITIONS)
            .expect("poll offered event");
        if poll.status() == M11RecursiveGreenBuildStatus::NeedsInput {
            break;
        }
        assert_eq!(poll.status(), M11RecursiveGreenBuildStatus::Pending);
    }
}

fn close_runtime(runtime: &mut DocumentRuntime) {
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(64).expect("poll close").complete {}
}

fn structural_rebase_fixture(
    runtime: &mut DocumentRuntime,
) -> (
    M11RecursiveGreenRoot,
    M11RecursiveGreenStructuralBoundary,
    M11RecursiveGreenStructuralBoundary,
    M11RecursiveGreenStructuralBoundary,
    M11RecursiveGreenStructuralBoundary,
) {
    let lease = runtime.snapshot_current_source().expect("source lease");
    let mut build = M11RecursiveGreenBuild::new(runtime, lease).expect("Green build");
    offer(
        &mut build,
        runtime,
        M11RecursiveGreenEvent::Enter {
            frame: frame(1),
            kind: kind(1),
        },
    );
    let prefix = build
        .capture_structural_boundary()
        .expect("prefix boundary");
    let mut start = None;
    let mut end = None;
    let mut suffix = None;

    for (id, text_bytes) in [(2, 2_u64), (3, 2), (4, 2)] {
        offer(
            &mut build,
            runtime,
            M11RecursiveGreenEvent::Enter {
                frame: frame(id),
                kind: kind(2),
            },
        );
        offer(
            &mut build,
            runtime,
            M11RecursiveGreenEvent::Coverage {
                physical: metric(text_bytes, text_bytes),
                owner_depth: 0,
                part: M11RecursiveGreenCoveragePart::Content,
                logical: M11RecursiveGreenLogicalAction::Identity,
            },
        );
        offer(
            &mut build,
            runtime,
            M11RecursiveGreenEvent::Exit {
                frame: frame(id),
                final_kind: kind(2),
                close: None,
                last_line_blank: false,
                child: M11RecursiveGreenClosedChild::default(),
            },
        );
        let boundary = build.capture_structural_boundary().expect("child boundary");
        match id {
            2 => start = Some(boundary),
            3 => end = Some(boundary),
            4 => suffix = Some(boundary),
            _ => unreachable!(),
        }
    }
    offer(
        &mut build,
        runtime,
        M11RecursiveGreenEvent::Exit {
            frame: frame(1),
            final_kind: kind(1),
            close: None,
            last_line_blank: false,
            child: M11RecursiveGreenClosedChild::default(),
        },
    );
    build.finish_input().expect("finish Green input");
    loop {
        let poll = build.poll(runtime, 1).expect("poll Green finish");
        if poll.status() == M11RecursiveGreenBuildStatus::Complete {
            break;
        }
        assert_eq!(poll.status(), M11RecursiveGreenBuildStatus::Pending);
    }
    (
        build.take_root().expect("Green root"),
        prefix,
        start.expect("replacement start"),
        end.expect("replacement end"),
        suffix.expect("suffix boundary"),
    )
}

#[test]
fn structural_splice_rebase_preserves_prefix_shifts_suffix_and_rejects_wrong_base() {
    let mut runtime =
        DocumentRuntime::new("a\nb\nc\n", DocumentRuntimeConfig::default()).expect("runtime");
    let (mut base, prefix, start, end, suffix) = structural_rebase_fixture(&mut runtime);
    let (mut wrong_base, wrong_prefix, _, _, _) = structural_rebase_fixture(&mut runtime);
    let base_source = base.source();

    runtime
        .apply_edit(base_source, 2..4, "L\nM\n")
        .expect("length-changing balanced edit");
    let target_source = runtime.current_source_version().expect("target source");
    let target_lease = runtime.snapshot_current_source().expect("target lease");
    let unchanged_prefix = runtime
        .mint_exact_unchanged_prefix_witness(base_source, 2, 2)
        .expect("unchanged prefix");
    let unchanged_suffix = runtime
        .mint_exact_unchanged_suffix_witness(base_source, 4, 4)
        .expect("unchanged suffix");
    let replacement = [
        M11RecursiveGreenEvent::Enter {
            frame: frame(5),
            kind: kind(2),
        },
        M11RecursiveGreenEvent::Coverage {
            physical: metric(2, 2),
            owner_depth: 0,
            part: M11RecursiveGreenCoveragePart::Content,
            logical: M11RecursiveGreenLogicalAction::Identity,
        },
        M11RecursiveGreenEvent::Exit {
            frame: frame(5),
            final_kind: kind(2),
            close: None,
            last_line_blank: false,
            child: M11RecursiveGreenClosedChild::default(),
        },
        M11RecursiveGreenEvent::Enter {
            frame: frame(6),
            kind: kind(2),
        },
        M11RecursiveGreenEvent::Coverage {
            physical: metric(2, 2),
            owner_depth: 0,
            part: M11RecursiveGreenCoveragePart::Content,
            logical: M11RecursiveGreenLogicalAction::Identity,
        },
        M11RecursiveGreenEvent::Exit {
            frame: frame(6),
            final_kind: kind(2),
            close: None,
            last_line_blank: false,
            child: M11RecursiveGreenClosedChild::default(),
        },
    ];
    let (mut target, _, _, _, rebase) = splice_m11_recursive_green_structural_atomic(
        &mut runtime,
        &base,
        target_lease,
        Some(unchanged_prefix),
        Some(unchanged_suffix),
        start,
        end,
        metric(6, 6),
        &replacement,
    )
    .expect("balanced structural splice");

    let target_prefix = rebase.rebase_prefix(prefix).expect("rebase prefix");
    assert_eq!(target_prefix.source(), target_source);
    assert_eq!(target_prefix.event_cut(), 1);
    assert_eq!(target_prefix.physical_metric(), metric(0, 0));
    assert_eq!(target_prefix.logical_metric(), metric(0, 0));

    let target_suffix = rebase.rebase_suffix(suffix).expect("rebase suffix");
    assert_eq!(target_suffix.source(), target_source);
    assert_eq!(target_suffix.event_cut(), 13);
    assert_eq!(target_suffix.physical_metric(), metric(8, 8));
    assert_eq!(target_suffix.logical_metric(), metric(8, 8));

    assert!(matches!(
        rebase.rebase_prefix(wrong_prefix),
        Err(M11RecursiveGreenError::SourceAuthorityMismatch)
    ));

    for root in [&mut target, &mut base, &mut wrong_base] {
        root.begin_release(&mut runtime).expect("release root");
        while !root
            .poll_release(&mut runtime, 64)
            .expect("poll root release")
            .complete()
        {}
    }
    close_runtime(&mut runtime);
}

#[test]
fn active_terminal_fragment_cursor_and_visible_suffix_rewrite_preserve_projection_and_source() {
    const SOURCE: &str = "> \t[x]: /u\r\nvisible";
    let mut runtime =
        DocumentRuntime::new(SOURCE, DocumentRuntimeConfig::default()).expect("runtime");
    let lease = runtime.snapshot_current_source().expect("source lease");
    let mut build = M11RecursiveGreenBuild::new(&runtime, lease).expect("green build");

    for (id, value) in [(1, 1), (2, 2), (3, 3)] {
        offer(
            &mut build,
            &mut runtime,
            M11RecursiveGreenEvent::Enter {
                frame: frame(id),
                kind: kind(value),
            },
        );
    }
    let fragment = build
        .mint_terminal_fragment(frame(3))
        .expect("mint active terminal fragment");
    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Coverage {
            physical: metric(2, 2),
            owner_depth: 2,
            part: M11RecursiveGreenCoveragePart::ContainerMarker,
            logical: M11RecursiveGreenLogicalAction::None,
        },
    );
    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Coverage {
            physical: metric(1, 1),
            owner_depth: 1,
            part: M11RecursiveGreenCoveragePart::ContainerMarker,
            logical: M11RecursiveGreenLogicalAction::PartialTab {
                target_owner_depth: 0,
                remaining_spaces: 3,
            },
        },
    );
    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Coverage {
            physical: metric(7, 7),
            owner_depth: 0,
            part: M11RecursiveGreenCoveragePart::Content,
            logical: M11RecursiveGreenLogicalAction::Identity,
        },
    );
    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Coverage {
            physical: metric(2, 2),
            owner_depth: 0,
            part: M11RecursiveGreenCoveragePart::Terminal,
            logical: M11RecursiveGreenLogicalAction::CanonicalNewline,
        },
    );
    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Coverage {
            physical: metric(7, 7),
            owner_depth: 0,
            part: M11RecursiveGreenCoveragePart::Content,
            logical: M11RecursiveGreenLogicalAction::Identity,
        },
    );

    build
        .begin_terminal_fragment_barrier(fragment)
        .expect("begin force-seal barrier");
    loop {
        let poll = build
            .poll_terminal_fragment_barrier(&mut runtime, 1)
            .expect("poll force-seal barrier");
        assert!(poll.transitions() <= 1);
        if poll.status() == M11RecursiveGreenTerminalFragmentBarrierStatus::Ready {
            break;
        }
    }
    let binding = build
        .take_terminal_fragment_binding()
        .expect("take frozen fragment binding");

    let mut cursor = build
        .open_terminal_fragment_cursor(&binding)
        .expect("open logical cursor");
    let mut projected = Vec::new();
    loop {
        match build
            .poll_terminal_fragment_cursor(&mut runtime, &mut cursor, 1)
            .expect("poll logical cursor")
            .status()
        {
            M11RecursiveGreenTerminalFragmentCursorStatus::Pending => {}
            M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady => {
                let ready = cursor.ready_byte().expect("ready projection byte");
                let offset = ready.relative_offset();
                projected.push(cursor.read_byte(offset).expect("read projected byte"));
                if offset == 0 {
                    assert_eq!(cursor.raw_codepoint_contribution(offset), 1);
                }
                if offset == 1 || offset == 2 {
                    assert_eq!(cursor.raw_codepoint_contribution(offset), 0);
                }
                if offset == 10 {
                    assert_eq!(cursor.raw_codepoint_contribution(offset), 2);
                }
            }
            M11RecursiveGreenTerminalFragmentCursorStatus::Complete => break,
        }
    }
    assert_eq!(projected, b"   [x]: /u\nvisible");

    let mut chunk_cursor = build
        .open_terminal_fragment_cursor(&binding)
        .expect("open chunked logical cursor");
    let mut chunk_projected = Vec::new();
    loop {
        let poll = build
            .poll_terminal_fragment_cursor_chunk(&mut runtime, &mut chunk_cursor, 1)
            .expect("poll chunked logical cursor");
        assert!(poll.transitions() <= 1);
        match poll.status() {
            M11RecursiveGreenTerminalFragmentCursorStatus::Pending => {}
            M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady => {
                let ready = chunk_cursor.ready_chunk();
                assert!(!ready.is_empty());
                assert!(ready.len() <= SOURCE_CURSOR_WINDOW_BYTES);
                let ready_len = ready.len();
                chunk_projected.extend_from_slice(ready);
                chunk_cursor
                    .consume_ready_prefix(ready_len)
                    .expect("consume projected chunk");
            }
            M11RecursiveGreenTerminalFragmentCursorStatus::Complete => break,
        }
    }
    assert_eq!(chunk_projected, projected);

    let prefix = build
        .bind_terminal_fragment_logical_range(
            &binding,
            M11RecursiveGreenLogicalRange::new(
                M11RecursiveGreenLogicalPosition::new(0, 0).unwrap(),
                M11RecursiveGreenLogicalPosition::new(11, 11).unwrap(),
            )
            .unwrap(),
        )
        .expect("bind accepted definition prefix");
    let mut replay = build
        .open_terminal_fragment_range_replay(&binding, prefix)
        .expect("open bounded prefix replay");
    let mut replayed = Vec::new();
    loop {
        match build
            .poll_terminal_fragment_cursor(&mut runtime, &mut replay, 1)
            .expect("poll range replay")
            .status()
        {
            M11RecursiveGreenTerminalFragmentCursorStatus::Pending => {}
            M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady => {
                let ready = replay.ready_byte().unwrap();
                replayed.push(replay.read_byte(ready.relative_offset()).unwrap());
            }
            M11RecursiveGreenTerminalFragmentCursorStatus::Complete => break,
        }
    }
    assert_eq!(replayed, b"   [x]: /u\n");
    let prefix = replay
        .take_completed_range()
        .expect("range replay authenticates both cuts");
    let prefix_physical_end = prefix
        .physical_range()
        .expect("prefix has physical authority")
        .byte_range()
        .end;
    let empty = build
        .bind_terminal_fragment_logical_range(
            &binding,
            M11RecursiveGreenLogicalRange::new(
                M11RecursiveGreenLogicalPosition::new(11, 11).unwrap(),
                M11RecursiveGreenLogicalPosition::new(11, 11).unwrap(),
            )
            .unwrap(),
        )
        .expect("bind empty range at the monotonic cursor");
    build
        .retarget_terminal_fragment_range_replay_forward(&binding, &mut replay, empty)
        .expect("retarget completed replay to adjacent empty range");
    assert_eq!(
        build
            .poll_terminal_fragment_cursor(&mut runtime, &mut replay, 1)
            .expect("poll empty range")
            .status(),
        M11RecursiveGreenTerminalFragmentCursorStatus::Complete
    );
    let empty = replay
        .take_completed_range()
        .expect("empty range retains exact point authority");
    assert_eq!(
        empty
            .physical_range()
            .expect("empty range has physical point")
            .byte_range(),
        prefix_physical_end..prefix_physical_end
    );
    let mut rewrite = build
        .begin_terminal_fragment_rewrite(
            &mut runtime,
            binding,
            M11RecursiveGreenTerminalFragmentRewrite::RetainVisibleSuffix {
                removed_prefix: prefix,
            },
        )
        .expect("begin canonical suffix rewrite");
    loop {
        match build
            .poll_terminal_fragment_rewrite(&mut runtime, &mut rewrite, 1)
            .expect("poll canonical suffix rewrite")
        {
            M11RecursiveGreenTerminalFragmentRewritePoll::Pending { transitions } => {
                assert!(transitions <= 1);
            }
            M11RecursiveGreenTerminalFragmentRewritePoll::Complete {
                transitions,
                mut authority,
            } => {
                assert!(transitions <= 1);
                assert_eq!(authority.frame(), frame(3));
                assert_eq!(
                    authority.disposition(),
                    M11RecursiveGreenTerminalFragmentDisposition::Surviving
                );
                let boundary = authority
                    .take_visible_remainder_boundary()
                    .expect("retain rewrite authenticates the definition/remainder cut");
                assert_eq!(boundary.physical_metric().bytes(), 12);
                assert_eq!(boundary.physical_metric().utf16(), 12);
                assert_eq!(boundary.logical_metric().bytes(), 0);
                assert_eq!(boundary.logical_metric().utf16(), 0);
                assert_eq!(boundary.open_path().len(), 3);
                break;
            }
        }
    }

    for (id, value) in [(3, 3), (2, 2), (1, 1)] {
        offer(
            &mut build,
            &mut runtime,
            M11RecursiveGreenEvent::Exit {
                frame: frame(id),
                final_kind: kind(value),
                close: None,
                last_line_blank: false,
                child: M11RecursiveGreenClosedChild::default(),
            },
        );
    }
    build.finish_input().expect("finish rewritten Green input");
    loop {
        let poll = build.poll(&mut runtime, 1).expect("seal rewritten Green");
        if poll.status() == M11RecursiveGreenBuildStatus::Complete {
            break;
        }
        assert_eq!(poll.status(), M11RecursiveGreenBuildStatus::Pending);
    }
    let mut root = build.take_root().expect("rewritten Green root");
    assert_eq!(root.source_byte_len(), SOURCE.len() as u64);
    assert_eq!(root.logical_byte_len(), 7);
    let definition = root
        .locate_point(
            &runtime,
            M11RecursiveGreenPoint::new(3, 3, SourceBoundaryAffinity::After),
        )
        .expect("definition point")
        .expect("definition coverage");
    assert_eq!(definition.owner().frame(), frame(2));
    assert_eq!(definition.part(), M11RecursiveGreenCoveragePart::Gap);
    let visible = root
        .locate_point(
            &runtime,
            M11RecursiveGreenPoint::new(12, 12, SourceBoundaryAffinity::After),
        )
        .expect("visible point")
        .expect("visible coverage");
    assert_eq!(visible.owner().frame(), frame(3));
    assert_eq!(
        visible.logical_atom(),
        M11RecursiveGreenLogicalAtom::Identity
    );

    root.begin_release(&mut runtime).expect("release root");
    while !root
        .poll_release(&mut runtime, 64)
        .expect("poll root release")
        .complete()
    {}
    close_runtime(&mut runtime);
}

#[test]
fn source_bound_build_atomizes_canonical_text_and_seals_with_fuel_one() {
    let mut runtime =
        DocumentRuntime::new("* a\0b\n", DocumentRuntimeConfig::default()).expect("runtime");
    let lease = runtime.snapshot_current_source().expect("source lease");
    let mut build = M11RecursiveGreenBuild::new(&runtime, lease).expect("green build");

    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Enter {
            frame: frame(1),
            kind: kind(1),
        },
    );
    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Enter {
            frame: frame(2),
            kind: kind(2),
        },
    );
    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Enter {
            frame: frame(3),
            kind: kind(3),
        },
    );
    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Coverage {
            physical: metric(2, 2),
            owner_depth: 0,
            part: M11RecursiveGreenCoveragePart::BlockMarker,
            logical: M11RecursiveGreenLogicalAction::None,
        },
    );
    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Enter {
            frame: frame(4),
            kind: kind(4),
        },
    );
    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Coverage {
            physical: metric(3, 3),
            owner_depth: 0,
            part: M11RecursiveGreenCoveragePart::Content,
            logical: M11RecursiveGreenLogicalAction::CanonicalText,
        },
    );
    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Coverage {
            physical: metric(1, 1),
            owner_depth: 0,
            part: M11RecursiveGreenCoveragePart::Terminal,
            logical: M11RecursiveGreenLogicalAction::CanonicalNewline,
        },
    );
    offer(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::RetypeOpen {
            frame: frame(4),
            kind: kind(9),
            property: None,
        },
    );
    for (id, block_kind) in [(4, 9), (3, 3), (2, 2), (1, 1)] {
        offer(
            &mut build,
            &mut runtime,
            M11RecursiveGreenEvent::Exit {
                frame: frame(id),
                final_kind: kind(block_kind),
                close: None,
                last_line_blank: false,
                child: M11RecursiveGreenClosedChild::default(),
            },
        );
    }

    build.finish_input().expect("finish green input");
    loop {
        let poll = build.poll(&mut runtime, 1).expect("finish build");
        if poll.status() == M11RecursiveGreenBuildStatus::Complete {
            break;
        }
        assert_eq!(poll.status(), M11RecursiveGreenBuildStatus::Pending);
    }
    let mut root = build.take_root().expect("green root");
    assert_eq!(root.source_byte_len(), 6);
    assert_eq!(root.source_utf16_len(), 6);
    assert_eq!(root.logical_byte_len(), 6);
    assert_eq!(root.logical_utf16_len(), 4);
    assert!(
        root.event_count() > 12,
        "NUL creates a distinct projection atom"
    );
    assert_eq!(root.build_receipt().storage_pages(), 1);
    let location = root
        .locate_point(
            &runtime,
            M11RecursiveGreenPoint::new(3, 3, SourceBoundaryAffinity::After),
        )
        .expect("point query")
        .expect("NUL coverage");
    assert_eq!(location.byte_range(), 3..4);
    assert_eq!(
        location.logical_atom(),
        M11RecursiveGreenLogicalAtom::NulToReplacement
    );
    assert_eq!(location.logical_metric(), metric(3, 1));
    assert_eq!(location.owner().frame(), frame(4));
    assert_eq!(
        location.owner().kind(),
        kind(9),
        "close-time retype is final"
    );
    assert_eq!(
        location
            .ancestry()
            .iter()
            .map(|ancestor| ancestor.kind())
            .collect::<Vec<_>>(),
        vec![kind(1), kind(2), kind(3), kind(9)]
    );
    assert_eq!(location.receipt().storage_pages_visited(), 1);

    root.begin_release(&mut runtime)
        .expect("begin root release");
    while !root
        .poll_release(&mut runtime, 64)
        .expect("poll root release")
        .complete()
    {}
    close_runtime(&mut runtime);
}

fn build_point_query_scale_fixture(
    item_count: usize,
) -> (
    DocumentRuntime,
    super::M11RecursiveGreenRoot,
    M11RecursiveGreenPoint,
) {
    let source = "x".repeat(item_count);
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let lease = runtime.snapshot_current_source().expect("source lease");
    let mut build = M11RecursiveGreenBuild::new(&runtime, lease).expect("green build");
    let target = item_count / 2;

    offer_fast(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Enter {
            frame: frame(1),
            kind: kind(1),
        },
    );
    for index in 0..item_count {
        let item_frame = 2 + u64::try_from(index).expect("item frame") * 2;
        offer_fast(
            &mut build,
            &mut runtime,
            M11RecursiveGreenEvent::Enter {
                frame: frame(item_frame),
                kind: kind(2),
            },
        );
        if index == target {
            offer_fast(
                &mut build,
                &mut runtime,
                M11RecursiveGreenEvent::Enter {
                    frame: frame(item_frame + 1),
                    kind: kind(3),
                },
            );
        }
        offer_fast(
            &mut build,
            &mut runtime,
            M11RecursiveGreenEvent::Coverage {
                physical: metric(1, 1),
                owner_depth: 0,
                part: M11RecursiveGreenCoveragePart::Content,
                logical: M11RecursiveGreenLogicalAction::Identity,
            },
        );
        if index == target {
            offer_fast(
                &mut build,
                &mut runtime,
                M11RecursiveGreenEvent::RetypeOpen {
                    frame: frame(item_frame + 1),
                    kind: kind(4),
                    property: None,
                },
            );
            offer_fast(
                &mut build,
                &mut runtime,
                M11RecursiveGreenEvent::Exit {
                    frame: frame(item_frame + 1),
                    final_kind: kind(4),
                    close: None,
                    last_line_blank: false,
                    child: M11RecursiveGreenClosedChild::default(),
                },
            );
        }
        offer_fast(
            &mut build,
            &mut runtime,
            M11RecursiveGreenEvent::Exit {
                frame: frame(item_frame),
                final_kind: kind(2),
                close: None,
                last_line_blank: false,
                child: M11RecursiveGreenClosedChild::default(),
            },
        );
    }
    offer_fast(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::RetypeOpen {
            frame: frame(1),
            kind: kind(9),
            property: None,
        },
    );
    offer_fast(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Exit {
            frame: frame(1),
            final_kind: kind(9),
            close: None,
            last_line_blank: false,
            child: M11RecursiveGreenClosedChild::default(),
        },
    );
    build.finish_input().expect("finish Green input");
    loop {
        let poll = build
            .poll(
                &mut runtime,
                super::M11_RECURSIVE_GREEN_MAX_POLL_TRANSITIONS,
            )
            .expect("finish Green build");
        if poll.status() == M11RecursiveGreenBuildStatus::Complete {
            break;
        }
        assert_eq!(poll.status(), M11RecursiveGreenBuildStatus::Pending);
    }
    let root = build.take_root().expect("Green root");
    let point = M11RecursiveGreenPoint::new(target, target, SourceBoundaryAffinity::After);
    (runtime, root, point)
}

fn assert_point_location_semantics_equal(
    zipper: &super::M11RecursiveGreenLocation,
    linear: &super::M11RecursiveGreenLocation,
) {
    assert_eq!(zipper.byte_range(), linear.byte_range());
    assert_eq!(zipper.utf16_range(), linear.utf16_range());
    assert_eq!(zipper.physical_metric(), linear.physical_metric());
    assert_eq!(zipper.logical_metric(), linear.logical_metric());
    assert_eq!(zipper.part(), linear.part());
    assert_eq!(zipper.logical_atom(), linear.logical_atom());
    assert_eq!(zipper.owner_index(), linear.owner_index());
    assert_eq!(zipper.ancestry(), linear.ancestry());
}

#[test]
fn point_zipper_matches_linear_oracle_and_is_document_size_independent() {
    let (mut small_runtime, mut small_root, small_point) = build_point_query_scale_fixture(256);
    let (mut large_runtime, mut large_root, large_point) = build_point_query_scale_fixture(8192);

    let small_zipper = small_root
        .locate_point(&small_runtime, small_point)
        .expect("small zipper query")
        .expect("small zipper location");
    let small_linear = super::query::locate_point_in_arena_linear(
        small_runtime.producer_arena(),
        small_root.tree.as_ref().expect("small tree").as_ref(),
        small_root.summary,
        small_point,
    )
    .expect("small linear query")
    .expect("small linear location");
    assert_point_location_semantics_equal(&small_zipper, &small_linear);

    let large_zipper = large_root
        .locate_point(&large_runtime, large_point)
        .expect("large zipper query")
        .expect("large zipper location");
    let large_linear = super::query::locate_point_in_arena_linear(
        large_runtime.producer_arena(),
        large_root.tree.as_ref().expect("large tree").as_ref(),
        large_root.summary,
        large_point,
    )
    .expect("large linear query")
    .expect("large linear location");
    assert_point_location_semantics_equal(&large_zipper, &large_linear);

    assert_eq!(large_zipper.owner().kind(), kind(4));
    assert_eq!(large_zipper.ancestry()[0].kind(), kind(9));
    assert!(
        large_zipper.receipt().storage_pages_visited()
            <= small_zipper.receipt().storage_pages_visited() + 2,
        "zipper leaf work should depend on ancestry depth, not document length: small={:?}, large={:?}",
        small_zipper.receipt(),
        large_zipper.receipt(),
    );
    assert!(
        large_linear.receipt().storage_pages_visited()
            > large_zipper.receipt().storage_pages_visited() * 16,
        "the discriminating fixture must expose the old document-wide scan: zipper={:?}, linear={:?}",
        large_zipper.receipt(),
        large_linear.receipt(),
    );
    assert!(
        large_zipper.receipt().node_headers_decoded() <= 256,
        "the 8,192-item zipper must fit the product host tree-node budget: {:?}",
        large_zipper.receipt(),
    );

    small_root
        .begin_release(&mut small_runtime)
        .expect("release small root");
    while !small_root
        .poll_release(&mut small_runtime, 64)
        .expect("poll small root release")
        .complete()
    {}
    close_runtime(&mut small_runtime);
    large_root
        .begin_release(&mut large_runtime)
        .expect("release large root");
    while !large_root
        .poll_release(&mut large_runtime, 64)
        .expect("poll large root release")
        .complete()
    {}
    close_runtime(&mut large_runtime);
}

#[test]
fn point_zipper_tree_node_fuel_is_exact_and_returns_no_partial_location() {
    let (mut runtime, mut root, point) = build_point_query_scale_fixture(2048);
    let baseline = root
        .locate_point(&runtime, point)
        .expect("unbounded point query")
        .expect("point location");
    let exact_limit = baseline.receipt().node_headers_decoded();
    assert!(
        exact_limit > 1,
        "fixture must require multiple node headers"
    );

    let exact = super::query::locate_point_in_arena_bounded(
        runtime.producer_arena(),
        root.tree.as_ref().expect("point tree").as_ref(),
        root.summary,
        point,
        exact_limit,
    )
    .expect("exact-budget point query");
    let exact = match exact {
        super::M11RecursiveGreenPointQueryOutcome::Location(location) => location,
        other => panic!("exact budget must return the complete location: {other:?}"),
    };
    assert_point_location_semantics_equal(&exact, &baseline);
    assert_eq!(exact.receipt().node_headers_decoded(), exact_limit);

    let one_short = super::query::locate_point_in_arena_bounded(
        runtime.producer_arena(),
        root.tree.as_ref().expect("point tree").as_ref(),
        root.summary,
        point,
        exact_limit - 1,
    )
    .expect("one-short point query returns a typed outcome");
    let exceeded = match one_short {
        super::M11RecursiveGreenPointQueryOutcome::BudgetExceeded(exceeded) => exceeded,
        other => panic!("one-short budget must return only budget evidence: {other:?}"),
    };
    assert_eq!(exceeded.receipt().node_headers_decoded(), exact_limit - 1);

    root.begin_release(&mut runtime)
        .expect("release point root");
    while !root
        .poll_release(&mut runtime, 64)
        .expect("poll point root release")
        .complete()
    {}
    close_runtime(&mut runtime);
}

fn fold_events(events: &[PackedGreenEvent]) -> RecursiveGreenSummary {
    events
        .iter()
        .copied()
        .fold(RecursiveGreenSummary::empty(), |summary, event| {
            summary
                .checked_followed_by(packed_event_summary(event).expect("event summary"))
                .expect("summary composition")
        })
}

#[test]
fn canonical_event_commitment_is_shape_independent_and_byte_exact() {
    let property = super::M11RecursiveGreenPropertyChunk::new(
        M11RecursiveGreenFactTag::new(41).expect("property tag"),
        b"alpha",
    )
    .expect("property");
    let replacement_property = super::M11RecursiveGreenPropertyChunk::new(
        M11RecursiveGreenFactTag::new(41).expect("replacement property tag"),
        b"bravo",
    )
    .expect("replacement property");
    let close = M11RecursiveGreenCloseFacts::new(
        M11RecursiveGreenFactTag::new(42).expect("close tag"),
        b"omega",
    )
    .expect("close facts");
    let events = [
        PackedGreenEvent::Enter {
            frame: frame(1),
            kind: kind(1),
        },
        PackedGreenEvent::Property(property),
        PackedGreenEvent::Coverage {
            physical: metric(4, 4),
            owner_depth: 0,
            part: M11RecursiveGreenCoveragePart::Content,
            atom: LogicalAtom::Identity,
        },
        PackedGreenEvent::RetypeOpen {
            frame: frame(1),
            kind: kind(2),
            property: None,
        },
        PackedGreenEvent::Exit {
            frame: frame(1),
            final_kind: kind(2),
            close: Some(close),
            last_line_blank: false,
            child: M11RecursiveGreenClosedChild::new(false, false, false),
        },
    ];
    let whole = fold_events(&events);
    let regrouped = fold_events(&events[..2])
        .checked_followed_by(fold_events(&events[2..]))
        .expect("regrouped summary");
    assert_eq!(whole, regrouped);
    assert_eq!(
        whole.canonical_event_bytes,
        events
            .iter()
            .copied()
            .map(packed_event_len)
            .map(|len| u64::try_from(len).expect("event length"))
            .sum()
    );

    let mut changed = events;
    changed[1] = PackedGreenEvent::Property(replacement_property);
    let changed = fold_events(&changed);
    assert_eq!(changed.canonical_event_bytes, whole.canonical_event_bytes);
    assert_ne!(
        changed.canonical_commitment.checksum(),
        whole.canonical_commitment.checksum(),
        "same-length canonical event changes must alter the commitment"
    );

    let mut page = [0_u8; ARENA_PAGE_BYTES];
    let mut cursor = super::codec::GREEN_LEAF_HEADER_BYTES;
    for event in events.iter().copied() {
        encode_packed_event(event, &mut page, &mut cursor).expect("encode canonical event");
    }
    encode_leaf_header(
        &mut page,
        u16::try_from(events.len()).expect("event count"),
        cursor - super::codec::GREEN_LEAF_HEADER_BYTES,
        whole,
    )
    .expect("encode leaf header");
    let mut inspection = SequenceSpecInspection::default();
    let decoded = decode_leaf(&page[..cursor], &mut inspection)
        .expect("decode leaf")
        .expect("recursive-Green leaf");
    assert_eq!(decoded.summary, whole);
}

#[test]
fn depth_aware_child_fold_keeps_siblings_and_excludes_nested_descendants() {
    let nested = M11RecursiveGreenClosedChild::new(true, true, true);
    let first = M11RecursiveGreenClosedChild::new(false, true, false);
    let second = M11RecursiveGreenClosedChild::new(true, false, true);
    let events = [
        PackedGreenEvent::Enter {
            frame: frame(10),
            kind: kind(10),
        },
        PackedGreenEvent::Enter {
            frame: frame(11),
            kind: kind(11),
        },
        PackedGreenEvent::Exit {
            frame: frame(11),
            final_kind: kind(11),
            close: None,
            last_line_blank: true,
            child: nested,
        },
        PackedGreenEvent::Exit {
            frame: frame(10),
            final_kind: kind(10),
            close: None,
            last_line_blank: false,
            child: first,
        },
        PackedGreenEvent::Enter {
            frame: frame(12),
            kind: kind(12),
        },
        PackedGreenEvent::Exit {
            frame: frame(12),
            final_kind: kind(12),
            close: None,
            last_line_blank: true,
            child: second,
        },
    ];
    let whole = fold_events(&events);
    let mut expected = super::M11RecursiveGreenChildFold::default();
    expected.push(first);
    expected.push(second);
    assert_eq!(whole.minimum_closed_depth, Some(0));
    assert_eq!(whole.outermost_children, expected);

    for split in 0..=events.len() {
        let left = fold_events(&events[..split]);
        let right = fold_events(&events[split..]);
        assert_eq!(
            left.checked_followed_by(right).expect("partition fold"),
            whole,
            "summary must remain shape-independent at split {split}"
        );
    }
}

#[test]
fn close_facts_round_trip_full_capacity_without_narrowing_offsets() {
    let mut payload = [0_u8; super::M11_RECURSIVE_GREEN_CLOSE_FACTS_MAX_BYTES];
    payload[0] = 1;
    let offsets = [
        u64::from(u32::MAX) + 1,
        u64::from(u32::MAX) + 2,
        u64::from(u32::MAX) + 3,
        u64::MAX - 2,
        u64::MAX - 1,
        u64::MAX,
    ];
    for (index, offset) in offsets.into_iter().enumerate() {
        let start = 1 + index * 8;
        payload[start..start + 8].copy_from_slice(&offset.to_le_bytes());
    }
    for (index, byte) in payload[49..].iter_mut().enumerate() {
        *byte = u8::try_from(index + 1).expect("bounded trailer byte");
    }
    let close = M11RecursiveGreenCloseFacts::new(
        M11RecursiveGreenFactTag::new(7).expect("nonzero fact tag"),
        &payload,
    )
    .expect("maximum-sized close facts");
    let event = PackedGreenEvent::Exit {
        frame: frame(u64::MAX),
        final_kind: kind(u16::MAX),
        close: Some(close),
        last_line_blank: true,
        child: M11RecursiveGreenClosedChild::new(true, false, true),
    };

    let mut page = [0_u8; ARENA_PAGE_BYTES];
    let mut encode_cursor = 0;
    encode_packed_event(event, &mut page, &mut encode_cursor).expect("encode exit");
    assert_eq!(encode_cursor, packed_event_len(event));

    let mut decode_cursor = 0;
    let decoded =
        decode_packed_event(&page[..encode_cursor], &mut decode_cursor).expect("decode exit");
    assert_eq!(decode_cursor, encode_cursor);
    assert_eq!(decoded, event);
    let PackedGreenEvent::Exit {
        close: Some(decoded_close),
        last_line_blank,
        ..
    } = decoded
    else {
        panic!("decoded event must preserve exit close facts");
    };
    assert!(last_line_blank);
    assert_eq!(decoded_close.as_bytes(), payload);
    for (index, expected) in offsets.into_iter().enumerate() {
        let start = 1 + index * 8;
        assert_eq!(
            u64::from_le_bytes(
                decoded_close.as_bytes()[start..start + 8]
                    .try_into()
                    .expect("eight-byte offset"),
            ),
            expected,
        );
    }
}

#[test]
fn cached_row_close_facts_round_trip_without_expanding_event_enums() {
    let cached = M11RecursiveGreenCachedRowEditable::new(
        M11RecursiveGreenCachedRowEditCapability::Contiguous,
        metric(5, 5),
        metric(u64::from(u32::MAX) - 3, u64::from(u32::MAX) - 7),
    )
    .expect("ordered cached row geometry");
    let close = M11RecursiveGreenCloseFacts::new_with_cached_row_editable(
        M11RecursiveGreenFactTag::new(6).expect("nonzero row tag"),
        &[],
        cached,
    )
    .expect("cached row close facts");
    let (semantic, decoded) = close
        .cached_row_editable(0)
        .expect("decode cached row facts")
        .expect("cached row trailer");
    assert!(semantic.is_empty());
    assert_eq!(decoded, cached);
    assert!(close.as_bytes().len() < 24);
    let (semantic, split) = close
        .split_cached_row_editable()
        .expect("split cached row facts")
        .expect("split cached row trailer");
    assert!(semantic.is_empty());
    assert_eq!(split, cached);

    let oversized = M11RecursiveGreenCachedRowEditable::new(
        M11RecursiveGreenCachedRowEditCapability::Contiguous,
        metric(0, 0),
        metric(u64::from(u32::MAX) + 1, u64::from(u32::MAX) + 1),
    )
    .expect("ordered oversized geometry");
    assert!(matches!(
        M11RecursiveGreenCloseFacts::new_with_cached_row_editable(
            M11RecursiveGreenFactTag::new(6).expect("nonzero row tag"),
            &[],
            oversized,
        ),
        Err(M11RecursiveGreenError::InvalidEvent)
    ));

    let public_bytes = std::mem::size_of::<M11RecursiveGreenEvent>();
    let packed_bytes = std::mem::size_of::<PackedGreenEvent>();
    eprintln!("recursive_green_event_sizes public={public_bytes} packed={packed_bytes}");
    assert!(public_bytes <= 88);
    assert!(packed_bytes <= 88);
}

#[test]
fn length_changing_edit_in_20k_item_list_path_copies_only_local_storage() {
    const ITEMS: usize = 20_000;
    const ITEM_BYTES: usize = 4;
    let source = "- x\n".repeat(ITEMS);
    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("large-list runtime");
    let lease = runtime.snapshot_current_source().expect("base lease");
    let mut build = M11RecursiveGreenBuild::new(&runtime, lease).expect("green build");
    offer_fast(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Enter {
            frame: frame(1),
            kind: kind(1),
        },
    );
    offer_fast(
        &mut build,
        &mut runtime,
        M11RecursiveGreenEvent::Enter {
            frame: frame(2),
            kind: kind(2),
        },
    );
    for index in 0..ITEMS {
        let item_frame = frame(u64::try_from(index + 3).expect("item frame fits u64"));
        offer_fast(
            &mut build,
            &mut runtime,
            M11RecursiveGreenEvent::Enter {
                frame: item_frame,
                kind: kind(3),
            },
        );
        offer_fast(
            &mut build,
            &mut runtime,
            M11RecursiveGreenEvent::Coverage {
                physical: metric(ITEM_BYTES as u64, ITEM_BYTES as u64),
                owner_depth: 0,
                part: M11RecursiveGreenCoveragePart::Content,
                logical: M11RecursiveGreenLogicalAction::Identity,
            },
        );
        offer_fast(
            &mut build,
            &mut runtime,
            M11RecursiveGreenEvent::Exit {
                frame: item_frame,
                final_kind: kind(3),
                close: None,
                last_line_blank: false,
                child: M11RecursiveGreenClosedChild::default(),
            },
        );
    }
    for (id, block_kind) in [(2, 2), (1, 1)] {
        offer_fast(
            &mut build,
            &mut runtime,
            M11RecursiveGreenEvent::Exit {
                frame: frame(id),
                final_kind: kind(block_kind),
                close: None,
                last_line_blank: false,
                child: M11RecursiveGreenClosedChild::default(),
            },
        );
    }
    build.finish_input().expect("finish large list input");
    loop {
        let poll = build
            .poll(
                &mut runtime,
                super::M11_RECURSIVE_GREEN_MAX_POLL_TRANSITIONS,
            )
            .expect("finish large list build");
        if poll.status() == M11RecursiveGreenBuildStatus::Complete {
            break;
        }
        assert_eq!(poll.status(), M11RecursiveGreenBuildStatus::Pending);
    }
    let mut base = build.take_root().expect("large list root");
    assert_eq!(base.source_byte_len(), (ITEMS * ITEM_BYTES) as u64);
    assert!(base.storage_page_count() > 100);

    let far_suffix_ordinal = base.storage_page_count() - 2;
    let mut base_leaf_inspection = SequenceInspectionReceipt::default();
    let base_far_suffix = base
        .tree
        .as_ref()
        .expect("base tree")
        .as_ref()
        .locate_leaf_with_prefix(
            runtime.producer_arena(),
            far_suffix_ordinal,
            &mut base_leaf_inspection,
        )
        .expect("locate base suffix")
        .expect("base suffix leaf")
        .id;
    let base_tree_root = base.tree_root_id_for_test().expect("base tree root");
    let base_right_child = runtime
        .producer_arena()
        .child_at(base_tree_root, 1)
        .expect("base right root child");

    let edited_item = ITEMS / 4;
    let base_start = edited_item * ITEM_BYTES;
    let base_end = base_start + ITEM_BYTES;
    let replacement = "- longer\n";
    runtime
        .apply_edit(base.source(), base_start..base_end, replacement)
        .expect("length-changing item edit");
    let target_lease = runtime.snapshot_current_source().expect("target lease");
    let prefix = runtime
        .mint_exact_unchanged_prefix_witness(base.source(), base_start, base_start)
        .expect("exact unchanged prefix");
    let suffix = runtime
        .mint_exact_unchanged_suffix_witness(base.source(), base_end, base_end)
        .expect("exact unchanged suffix");
    let target_end = base_start + replacement.len();
    let (mut target, receipt) = splice_m11_recursive_green_coverage_atomic(
        &mut runtime,
        &base,
        target_lease,
        Some(prefix),
        Some(suffix),
        base_start..base_end,
        base_start..target_end,
        M11RecursiveGreenLogicalAction::Identity,
    )
    .expect("local recursive-green coverage splice");

    assert_eq!(target.source_byte_len(), base.source_byte_len() + 5);
    assert_eq!(target.storage_page_count(), base.storage_page_count());
    assert_eq!(receipt.deleted_storage_pages(), 1);
    assert_eq!(receipt.replacement_storage_pages(), 1);
    assert_eq!(
        receipt.reused_storage_pages(),
        base.storage_page_count() - 1,
    );
    assert_eq!(receipt.lineage_transitions(), 1);
    assert!(receipt.boundary_events_decoded() <= GREEN_EVENTS_PER_PAGE_MAX as u64,);
    assert!(receipt.boundary_events_reencoded() <= GREEN_EVENTS_PER_PAGE_MAX as u64,);
    let logarithmic_bound = usize::from(base.tree_height()) * 16 + 32;
    assert!(
        receipt.tree_nodes_visited() <= logarithmic_bound,
        "{} measured-tree visits exceeded height-derived bound {}",
        receipt.tree_nodes_visited(),
        logarithmic_bound,
    );
    assert!(receipt.branches_allocated() <= logarithmic_bound);

    let mut target_leaf_inspection = SequenceInspectionReceipt::default();
    let target_far_suffix = target
        .tree
        .as_ref()
        .expect("target tree")
        .as_ref()
        .locate_leaf_with_prefix(
            runtime.producer_arena(),
            far_suffix_ordinal,
            &mut target_leaf_inspection,
        )
        .expect("locate target suffix")
        .expect("target suffix leaf")
        .id;
    assert_eq!(target_far_suffix, base_far_suffix);
    let target_tree_root = target.tree_root_id_for_test().expect("target tree root");
    let target_right_child = runtime
        .producer_arena()
        .child_at(target_tree_root, 1)
        .expect("target right root child");
    assert_eq!(
        target_right_child, base_right_child,
        "the root child wholly outside the edited quarter must be retained",
    );

    target.begin_release(&mut runtime).expect("release target");
    base.begin_release(&mut runtime).expect("release base");
    while !target
        .poll_release(&mut runtime, 256)
        .expect("poll target release")
        .complete()
    {}
    close_runtime(&mut runtime);
}
