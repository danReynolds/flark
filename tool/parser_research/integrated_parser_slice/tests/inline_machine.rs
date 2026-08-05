use std::sync::Arc;
use std::time::Instant;

use flark_integrated_parser_slice::frontier::{
    CursorMetrics, LexerStatus, LexicalConsumers, SegmentedLeaf, SegmentedLeafBuilder, SharedLexer,
};
use flark_integrated_parser_slice::inline_machine::{
    InlineCancelStatus, InlineMachine, InlineOutputPageDrainStep, InlineSpanKind, InlineStatus,
    InlineWork, ProjectionStep, INLINE_OUTPUT_PAGE_BYTES, MAX_INLINE_ATOMIC_COPY_BYTES,
    MAX_INLINE_ATOMIC_PAGE_BYTES, MAX_INLINE_ATOMIC_PAGE_OPERATIONS,
};
use flark_integrated_parser_slice::source::PersistentSource;
use pulldown_cmark::{Event, Parser, Tag};

#[derive(Clone, Copy, Debug)]
struct LexerReceipt {
    work: usize,
    max_poll: usize,
    cursor: CursorMetrics,
}

fn lex(
    text: &str,
) -> (
    Arc<PersistentSource>,
    SegmentedLeaf,
    LexicalConsumers,
    LexerReceipt,
) {
    let source = Arc::new(PersistentSource::from_text(text));
    lex_source(source, text.len())
}

fn lex_owned(
    text: String,
) -> (
    Arc<PersistentSource>,
    SegmentedLeaf,
    LexicalConsumers,
    LexerReceipt,
) {
    let len = text.len();
    let source = Arc::new(PersistentSource::from_text(&text));
    drop(text);
    lex_source(source, len)
}

fn lex_source(
    source: Arc<PersistentSource>,
    len: usize,
) -> (
    Arc<PersistentSource>,
    SegmentedLeaf,
    LexicalConsumers,
    LexerReceipt,
) {
    let mut builder = SegmentedLeafBuilder::new(source.clone());
    if len > 0 {
        builder.push_source(0..len).unwrap();
    }
    let leaf = builder.finish();
    let mut lexer = SharedLexer::new(&leaf);
    loop {
        if lexer.poll(4096).status == LexerStatus::Ready {
            break;
        }
    }
    let receipt = LexerReceipt {
        work: lexer.total_work(),
        max_poll: lexer.max_poll_work(),
        cursor: lexer.cursor_metrics(),
    };
    assert!(receipt.max_poll <= 4096);
    assert_eq!(receipt.cursor.logical_bytes, len);
    assert!(receipt.work >= receipt.cursor.operations);
    (
        source,
        leaf,
        lexer.consumers().expect("ready lexer has consumers"),
        receipt,
    )
}

fn parse(text: &str, transitions: usize) -> (InlineMachine, Vec<(InlineSpanKind, usize, usize)>) {
    let (_, _, consumers, _) = lex(text);
    let mut machine = InlineMachine::new(consumers.inline);
    loop {
        let permit = InlineWork::uniform(transitions);
        let poll = machine.poll(permit);
        assert!(permit.allows(poll.delta));
        assert!(poll.delta.transitions <= transitions);
        if poll.status == InlineStatus::Ready {
            break;
        }
        assert!(poll.delta.transitions > 0, "permit must make progress");
    }
    let mut spans = machine
        .output()
        .expect("ready machine has output")
        .spans()
        .map(|span| (span.kind, span.start(), span.end()))
        .collect::<Vec<_>>();
    spans.sort_unstable();
    (machine, spans)
}

fn parse_cooperative(
    text: &str,
    transitions: usize,
) -> (InlineMachine, Vec<(InlineSpanKind, usize, usize)>) {
    let (_, _, consumers, _) = lex(text);
    let mut machine = InlineMachine::new(consumers.inline);
    loop {
        let poll = machine.poll_cooperative(transitions);
        assert!(poll.delta.transitions <= transitions);
        assert!(
            poll.delta.page_allocations
                <= poll.delta.transitions * MAX_INLINE_ATOMIC_PAGE_OPERATIONS
        );
        assert!(
            poll.delta.page_reclaims <= poll.delta.transitions * MAX_INLINE_ATOMIC_PAGE_OPERATIONS
        );
        assert!(
            poll.delta.allocated_bytes <= poll.delta.transitions * MAX_INLINE_ATOMIC_PAGE_BYTES
        );
        assert!(
            poll.delta.reclaimed_bytes <= poll.delta.transitions * MAX_INLINE_ATOMIC_PAGE_BYTES
        );
        assert!(poll.delta.copy_bytes <= poll.delta.transitions * MAX_INLINE_ATOMIC_COPY_BYTES);
        if poll.status == InlineStatus::Ready {
            break;
        }
        assert!(
            poll.delta.transitions > 0,
            "positive slice must make progress"
        );
    }
    let mut spans = output_spans_in_order(&machine);
    spans.sort_unstable();
    (machine, spans)
}

#[test]
fn a_missing_allocation_dimension_stops_before_allocating() {
    let (_, _, consumers, _) = lex("*a*");
    let mut machine = InlineMachine::new(consumers.inline);
    let mut blocked = false;
    for _ in 0..10_000 {
        let mut permit = InlineWork::uniform(1);
        permit.page_allocations = 0;
        permit.allocated_bytes = 0;
        let poll = machine.poll(permit);
        assert!(permit.allows(poll.delta));
        if poll.status == InlineStatus::Pending && poll.delta.transitions == 0 {
            blocked = true;
            break;
        }
    }
    assert!(blocked);
    assert_eq!(machine.retention().allocations, 0);
}

fn pulldown_spans_in_order(text: &str) -> Vec<(InlineSpanKind, usize, usize)> {
    Parser::new(text)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            let kind = match event {
                Event::Start(Tag::Emphasis) => InlineSpanKind::Emphasis,
                Event::Start(Tag::Strong) => InlineSpanKind::Strong,
                Event::Code(_) => InlineSpanKind::Code,
                _ => return None,
            };
            Some((kind, range.start, range.end))
        })
        .collect()
}

fn pulldown_spans(text: &str) -> Vec<(InlineSpanKind, usize, usize)> {
    let mut spans = pulldown_spans_in_order(text);
    spans.sort_unstable();
    spans
}

fn output_spans_in_order(machine: &InlineMachine) -> Vec<(InlineSpanKind, usize, usize)> {
    machine
        .output()
        .unwrap()
        .spans()
        .map(|span| (span.kind, span.start(), span.end()))
        .collect()
}

#[test]
fn exact_small_permit_never_hides_a_region_sized_transition() {
    let text = "before ***strong and emphasis*** plus `code *literal*` after";
    let (machine, actual) = parse(text, 1);
    assert_eq!(actual, pulldown_spans(text));
    let work = machine.total_work();
    assert!(work.transitions > text.len());
    assert!(work.delimiter_search_entries > 0);
    assert_eq!(work.emits, 3);
    assert!(work.copy_bytes > 0);
    assert!(work.hash_bytes > 0);
}

#[test]
fn cooperative_scalar_poll_is_output_and_receipt_equivalent_to_exact_poll() {
    let cases = [
        "",
        "plain text",
        "***nested*** and `code` then *tail*",
        "a `one ``ignored` two`` b ``three`` c `unmatched ``four``",
        "****a***b**c*",
        "_a _a _a _a _a ",
        "`code *literal*` and **strong _inside_**",
    ];
    for text in cases {
        let (exact, exact_spans) = parse(text, 13);
        let (cooperative, cooperative_spans) = parse_cooperative(text, 13);
        assert_eq!(cooperative_spans, exact_spans, "input {text:?}");
        let exact_output = exact.output().expect("exact output");
        let cooperative_output = cooperative.output().expect("cooperative output");
        assert_eq!(
            cooperative_output.spans().collect::<Vec<_>>(),
            exact_output.spans().collect::<Vec<_>>(),
            "source order for input {text:?}"
        );
        assert_eq!(cooperative_output.digest(), exact_output.digest());
        assert_eq!(
            cooperative_output.payload_bytes(),
            exact_output.payload_bytes()
        );
        assert_eq!(cooperative.total_work(), exact.total_work());
        assert_eq!(cooperative.total_telemetry(), exact.total_telemetry());
    }
}

#[test]
fn large_source_skip_distance_is_telemetry_not_scheduler_fuel() {
    const GAP: usize = 1024 * 1024;
    let text = format!("*a*{}*z*", "x".repeat(GAP));
    let source = Arc::new(PersistentSource::from_text(&text));
    let mut builder = SegmentedLeafBuilder::new(source);
    builder.push_source(0..3).unwrap();
    builder.push_virtual_newline(3).unwrap();
    builder.push_source(text.len() - 3..text.len()).unwrap();
    let leaf = builder.finish();
    let mut lexer = SharedLexer::new(&leaf);
    loop {
        if lexer.poll(4096).status == LexerStatus::Ready {
            break;
        }
    }
    let mut machine = InlineMachine::new(lexer.consumers().unwrap().inline);
    let permit = InlineWork::uniform(4096);
    let poll = machine.poll(permit);
    assert_eq!(poll.status, InlineStatus::Ready);
    assert!(permit.allows(poll.delta));
    assert_eq!(poll.telemetry_delta.source_skipped_bytes, GAP - 1);
    assert_eq!(machine.total_telemetry(), poll.telemetry_delta);
}

#[test]
fn donor_equivalent_code_index_handles_interleaved_run_lengths_linearly() {
    let text = "a `one ``ignored` two`` b ``three`` c `unmatched ``four``";
    let (machine, actual) = parse(text, 7);
    assert_eq!(actual, pulldown_spans(text));
    let work = machine.total_work();
    assert!(work.code_search_events >= machine.code_run_count());
    assert!(work.code_state_reads <= machine.code_run_count() * 3 + 1);
    assert!(work.code_state_writes <= machine.code_run_count() * 3 + actual.len() + 1);
}

#[test]
fn compact_stream_projects_nested_and_mixed_facts_in_canonical_source_order() {
    let text = "***nested*** and `code` then *tail*";
    let (mut machine, actual) = parse(text, 1);
    assert_eq!(actual, pulldown_spans(text));
    let raw = machine.output().unwrap().spans().collect::<Vec<_>>();
    assert_eq!(raw[0].kind, InlineSpanKind::Emphasis);
    assert_eq!(raw[1].kind, InlineSpanKind::Strong);
    assert_eq!(raw[2].kind, InlineSpanKind::Code);
    assert!(raw
        .windows(2)
        .all(|pair| pair[0].start() <= pair[1].start()));

    let output = machine.output().unwrap();
    let mut cursor = output.projection_cursor();
    let mut projected = Vec::new();
    loop {
        match cursor.step(output) {
            ProjectionStep::Progress => {}
            ProjectionStep::Span(span) => projected.push(span),
            ProjectionStep::Done => break,
        }
    }
    assert_eq!(projected, raw);
    assert_eq!(cursor.metrics().spans, raw.len());
    assert_eq!(cursor.metrics().decoded_bytes, output.payload_bytes());

    let standalone = machine.take_output().unwrap();
    drop(machine);
    assert_eq!(standalone.spans().collect::<Vec<_>>(), raw);
}

#[test]
fn generated_ascii_subset_differentials_match_pulldown() {
    const ALPHABET: &[u8] = b"a *_`";
    for ordinal in 0..4096usize {
        let mut value = ordinal;
        let mut sample = String::from("x");
        for _ in 0..6 {
            sample.push(char::from(ALPHABET[value % ALPHABET.len()]));
            value /= ALPHABET.len();
        }
        sample.push('y');
        let (machine, actual) = parse(&sample, 13);
        assert_eq!(actual, pulldown_spans(&sample), "input {sample:?}");
        assert_eq!(
            output_spans_in_order(&machine),
            pulldown_spans_in_order(&sample),
            "source order for input {sample:?}"
        );
    }
}

#[test]
fn deterministic_longer_ascii_differentials_match_pulldown() {
    // InlineMachine consumes one block leaf; paragraph-separating blank lines
    // belong to BlockJob and would make a direct whole-string differential an
    // invalid cross-layer test.
    const ALPHABET: &[u8] = b"ab 0* _`.!()";
    let alphabet_len = u64::try_from(ALPHABET.len()).expect("small alphabet fits u64");
    let mut state = 0x6a09_e667_f3bc_c909_u64;
    for ordinal in 0..2048usize {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let len = 8 + usize::try_from(state % 57).expect("bounded length fits usize");
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let index = usize::try_from(state % alphabet_len).expect("alphabet index fits usize");
            bytes.push(ALPHABET[index]);
        }
        let sample = format!("x{}y", String::from_utf8(bytes).expect("ASCII generator"));
        let (machine, actual) = parse_cooperative(&sample, 31);
        assert_eq!(
            actual,
            pulldown_spans(&sample),
            "sample {ordinal} input {sample:?}"
        );
        assert_eq!(
            output_spans_in_order(&machine),
            pulldown_spans_in_order(&sample),
            "source order sample {ordinal} input {sample:?}"
        );
    }
}

#[test]
fn curated_commonmark_delimiter_interactions_match_pulldown() {
    let cases = [
        "***foo***",
        "foo_bar_baz",
        "a***b**c*",
        "**foo *bar* baz**",
        "*foo **bar** baz*",
        "***foo** bar*",
        "*foo **bar***",
        "**foo *bar***",
        "`code *literal*` and *live*",
        "`` code ` tick `` and **strong**",
        "`unmatched ``matched`` then `closed`",
        "****a***b**c*",
        "x *soft\nwrapped* y",
        "x `code\nspan` and **strong\nspan** y",
    ];
    for text in cases {
        let (machine, actual) = parse(text, 1);
        assert_eq!(actual, pulldown_spans(text), "input {text:?}");
        assert_eq!(
            output_spans_in_order(&machine),
            pulldown_spans_in_order(text),
            "source order for input {text:?}"
        );
    }
}

#[test]
fn unmatched_openers_use_sixteen_raw_bytes_per_high_water_entry() {
    let repeats = 20_000;
    let text = "_a ".repeat(repeats);
    let (machine, actual) = parse(&text, 4096);
    assert!(actual.is_empty());
    let retained = machine.retention();
    assert_eq!(retained.delimiter_high_water, repeats);
    assert!(
        retained.peak_bytes < repeats * 18 + 128 * 1024,
        "{retained:?}"
    );
}

#[test]
fn completed_dense_output_is_compact_and_drops_the_role_overlay() {
    let repeats = 20_000;
    let text = "*a* ".repeat(repeats);
    let (machine, actual) = parse(&text, 4096);
    assert_eq!(actual.len(), repeats);
    let output = machine.output().unwrap();
    assert_eq!(output.payload_bytes(), repeats * 4);
    assert!(output.retained_bytes() < repeats * 5 + 256 * 1024);
    let retained = machine.retention();
    assert_eq!(retained.temporary_overlay_bytes, 0);
    assert_eq!(retained.fact_counter_bytes, 0);
    assert_eq!(machine.total_work().hash_bytes, output.payload_bytes());
}

#[test]
fn explicit_cancellation_reclaims_machine_pages_in_bounded_transitions() {
    let repeats = 20_000;
    let (_, _, consumers, _) = lex_owned("_a ".repeat(repeats));
    let mut machine = InlineMachine::new(consumers.inline);
    while machine.retention().delimiter_high_water < repeats {
        let poll = machine.poll_cooperative(4096);
        assert_eq!(poll.status, InlineStatus::Pending);
    }
    let peak_allocations = machine.retention().allocations;
    assert!(peak_allocations > 0);
    let mut transferred_allocations = 0usize;

    loop {
        let poll = machine.poll_cancel(7);
        assert!(poll.delta.transitions <= 7);
        assert!(
            poll.delta.page_reclaims <= poll.delta.transitions * MAX_INLINE_ATOMIC_PAGE_OPERATIONS
        );
        assert!(
            poll.delta.reclaimed_bytes <= poll.delta.transitions * MAX_INLINE_ATOMIC_PAGE_BYTES
        );
        if let Some(page) = poll.page {
            transferred_allocations += page.allocations();
            assert!(page.retained_bytes() <= MAX_INLINE_ATOMIC_PAGE_BYTES);
        }
        if poll.status == InlineCancelStatus::Ready {
            break;
        }
        assert!(poll.delta.transitions > 0);
    }
    let retained = machine.retention();
    assert_eq!(retained.allocations, 0);
    assert_eq!(retained.bytes, retained.fixed_machine_bytes);
    assert_eq!(transferred_allocations, peak_allocations);
    assert!(machine.output().is_none());
}

#[test]
fn consuming_page_drain_transfers_the_canonical_allocations_in_order() {
    let repeats = 2_000;
    let text = "*a* ".repeat(repeats);
    let (mut machine, actual) = parse(&text, 4096);
    assert_eq!(actual.len(), repeats);
    let output = machine.take_output().unwrap();
    let payload_bytes = output.payload_bytes();
    let digest = output.digest();
    let mut drain = output.into_page_drain();
    assert_eq!(drain.payload_bytes(), payload_bytes);
    assert_eq!(drain.digest(), digest);
    assert_eq!(drain.span_count(), repeats);

    let mut transferred = Vec::with_capacity(payload_bytes);
    while let InlineOutputPageDrainStep::Page(page) = drain.step() {
        assert!(page.used_len() <= INLINE_OUTPUT_PAGE_BYTES);
        transferred.extend_from_slice(page.as_bytes());
        let allocation = page.into_allocation();
        assert_eq!(allocation.len(), INLINE_OUTPUT_PAGE_BYTES);
    }
    assert_eq!(transferred.len(), payload_bytes);
    for (index, record) in transferred.chunks_exact(4).enumerate() {
        assert_eq!(record, [2, u8::from(index != 0) * 4, 1, 1]);
    }
    let metrics = drain.metrics();
    assert_eq!(metrics.page_transfers, payload_bytes.div_ceil(4096));
    assert_eq!(metrics.transferred_payload_bytes, payload_bytes);
    assert_eq!(metrics.output_index_steps, metrics.page_transfers * 4);
}

#[test]
#[ignore = "10 MiB release-mode architecture receipt"]
fn ten_mib_plain_and_giant_code_receipts() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    for text in ["a".repeat(TEN_MIB), format!("`{}`", "a".repeat(TEN_MIB))] {
        let (source, leaf, consumers, lexer_receipt) = lex(&text);
        let lexical_retained = consumers.inline.view().retained_event_bytes();
        let lexer_events = consumers.inline.view().event_count();
        let mut machine = InlineMachine::new(consumers.inline);
        loop {
            if machine.poll(InlineWork::uniform(4096)).status == InlineStatus::Ready {
                break;
            }
        }
        let source_retained = source.buffer_retention().retained_buffer_bytes;
        let descriptors = leaf.retained_descriptor_bytes().total();
        let machine_retained = machine.retention().bytes;
        eprintln!(
            "10MiB receipt events={lexer_events} source={source_retained} descriptors={descriptors} lexical={} machine={machine_retained} lexer={lexer_receipt:?} work={:?}",
            lexical_retained.total(),
            machine.total_work(),
        );
        assert_eq!(
            machine.output().unwrap().span_count(),
            usize::from(text.starts_with('`'))
        );
    }
}

#[test]
#[ignore = "10 MiB release-mode cancellation and memory receipt"]
fn ten_mib_delimiter_dense_cancellation_receipt() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    let text = "_a ".repeat(TEN_MIB / 3);
    let (source, leaf, consumers, lexer_receipt) = lex(&text);
    let lexical_retained = consumers.inline.view().retained_event_bytes();
    let mut machine = InlineMachine::new(consumers.inline);
    while machine.total_work().delimiter_classifications < 8192 {
        let poll = machine.poll(InlineWork::uniform(4096));
        assert_eq!(poll.status, InlineStatus::Pending);
        assert!(poll.delta.transitions <= 4096);
    }
    let retained = machine.retention();
    let whole_lower_bound = source.buffer_retention().retained_buffer_bytes
        + leaf.retained_descriptor_bytes().total()
        + lexical_retained.total()
        + retained.peak_bytes;
    eprintln!(
        "10MiB cancellation receipt whole_lower_bound={whole_lower_bound} machine={retained:?} lexer={lexer_receipt:?} work={:?}",
        machine.total_work(),
    );
    assert!(retained.delimiter_high_water <= 8192 + 4096);
    assert!(retained.bytes < 2 * 1024 * 1024);
}

#[test]
#[ignore = "10 MiB release-mode near-peak cancellation drop latency receipt"]
fn ten_mib_near_peak_cancellation_drop_latency_receipt() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    let repeats = TEN_MIB / 3;
    let (source, leaf, consumers, _) = lex_owned("_a ".repeat(repeats));
    let mut machine = InlineMachine::new(consumers.inline);
    while machine.retention().delimiter_high_water < repeats {
        let poll = machine.poll_cooperative(4096);
        assert_eq!(poll.status, InlineStatus::Pending);
    }
    let retained = machine.retention();
    assert_eq!(retained.delimiter_high_water, repeats);
    assert!(retained.delimiter_bytes > 50 * 1024 * 1024);

    // Keep the immutable source/lexical owners live so this first receipt
    // isolates synchronous destruction of the machine's radix scratch pages.
    let started = Instant::now();
    drop(machine);
    let machine_drop = started.elapsed();
    std::hint::black_box((&source, &leaf));
    eprintln!(
        "10MiB near-peak cancellation machine_drop_us={} retained={retained:?}",
        machine_drop.as_micros()
    );
}

#[test]
#[ignore = "10 MiB release-mode last-owner cancellation drop latency receipt"]
fn ten_mib_near_peak_last_owner_drop_latency_receipt() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    let repeats = TEN_MIB / 3;
    let (source, leaf, consumers, _) = lex_owned("_a ".repeat(repeats));
    let mut machine = InlineMachine::new(consumers.inline);
    drop(source);
    drop(leaf);
    while machine.retention().delimiter_high_water < repeats {
        let poll = machine.poll_cooperative(4096);
        assert_eq!(poll.status, InlineStatus::Pending);
    }
    let retained = machine.retention();
    assert_eq!(retained.delimiter_high_water, repeats);

    // This includes release of the machine pages plus the final immutable
    // lexical/source ownership chain, the production cancellation case.
    let started = Instant::now();
    drop(machine);
    let last_owner_drop = started.elapsed();
    eprintln!(
        "10MiB near-peak cancellation last_owner_drop_us={} retained={retained:?}",
        last_owner_drop.as_micros()
    );
}

#[test]
#[ignore = "10 MiB release-mode resumable cancellation latency receipt"]
fn ten_mib_resumable_cancellation_latency_receipt() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    let repeats = TEN_MIB / 3;
    let (source, leaf, consumers, _) = lex_owned("_a ".repeat(repeats));
    let mut machine = InlineMachine::new(consumers.inline);
    while machine.retention().delimiter_high_water < repeats {
        let poll = machine.poll_cooperative(4096);
        assert_eq!(poll.status, InlineStatus::Pending);
    }
    let peak = machine.retention();
    let mut polls = 0usize;
    let mut max_poll = std::time::Duration::ZERO;
    let mut pages = Vec::with_capacity(peak.allocations);
    let started = Instant::now();
    loop {
        let poll_started = Instant::now();
        let poll = machine.poll_cancel(64);
        max_poll = max_poll.max(poll_started.elapsed());
        polls += 1;
        assert!(poll.delta.transitions <= 64);
        let status = poll.status;
        if let Some(page) = poll.page {
            pages.push(page);
        }
        if status == InlineCancelStatus::Ready {
            break;
        }
    }
    let cleanup = started.elapsed();
    let retained = machine.retention();
    assert_eq!(retained.allocations, 0);
    assert_eq!(retained.bytes, retained.fixed_machine_bytes);

    let drop_started = Instant::now();
    drop(machine);
    let residual_drop = drop_started.elapsed();
    let reclaim_started = Instant::now();
    drop(pages);
    let off_lane_reclaim = reclaim_started.elapsed();
    std::hint::black_box((&source, &leaf));
    eprintln!(
        "10MiB resumable cancellation polls={polls} transfer_total_us={} max_poll_us={} residual_drop_us={} off_lane_reclaim_us={} peak={peak:?}",
        cleanup.as_micros(),
        max_poll.as_micros(),
        residual_drop.as_micros(),
        off_lane_reclaim.as_micros()
    );
}

#[test]
#[ignore = "10 MiB release-mode packed-state peak receipt"]
fn ten_mib_unmatched_delimiter_memory_receipt() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    const WHOLE_LOWER_BOUND_GATE: usize = 96 * 1024 * 1024;
    let repeats = TEN_MIB / 3;
    let text = "_a ".repeat(repeats);
    let (source, leaf, consumers, lexer_receipt) = lex(&text);
    let lexical_retained = consumers.inline.view().retained_event_bytes();
    let mut machine = InlineMachine::new(consumers.inline);
    loop {
        if machine.poll(InlineWork::uniform(4096)).status == InlineStatus::Ready {
            break;
        }
    }
    let retained = machine.retention();
    let whole_lower_bound = source.buffer_retention().retained_buffer_bytes
        + leaf.retained_descriptor_bytes().total()
        + lexical_retained.total()
        + retained.peak_bytes;
    eprintln!(
        "10MiB unmatched receipt whole_lower_bound={whole_lower_bound} machine={retained:?} lexer={lexer_receipt:?} work={:?}",
        machine.total_work(),
    );
    assert_eq!(retained.delimiter_high_water, repeats);
    assert_eq!(retained.output_spans, 0);
    assert!(whole_lower_bound < WHOLE_LOWER_BOUND_GATE);
}

#[test]
#[ignore = "10 MiB release-mode steady parser RSS receipt"]
fn ten_mib_unmatched_parser_steady_rss_receipt() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    let repeats = TEN_MIB / 3;
    let (source, leaf, consumers, lexer_receipt) = lex_owned("_a ".repeat(repeats));
    let lexical_retained = consumers.inline.view().retained_event_bytes();
    let mut machine = InlineMachine::new(consumers.inline);
    loop {
        if machine.poll(InlineWork::uniform(4096)).status == InlineStatus::Ready {
            break;
        }
    }
    let retained = machine.retention();
    let whole_lower_bound = source.buffer_retention().retained_buffer_bytes
        + leaf.retained_descriptor_bytes().total()
        + lexical_retained.total()
        + retained.peak_bytes;
    eprintln!(
        "10MiB unmatched steady-parser receipt whole_lower_bound={whole_lower_bound} machine={retained:?} lexer={lexer_receipt:?} work={:?}",
        machine.total_work(),
    );
    assert_eq!(retained.delimiter_high_water, repeats);
    assert_eq!(retained.output_spans, 0);
}

#[test]
#[ignore = "10 MiB release-mode packed-output peak receipt"]
fn ten_mib_dense_matched_output_memory_receipt() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    const WHOLE_LOWER_BOUND_GATE: usize = 96 * 1024 * 1024;
    let repeats = TEN_MIB / 4;
    let text = "*a* ".repeat(repeats);
    let (source, leaf, consumers, lexer_receipt) = lex(&text);
    let lexical_retained = consumers.inline.view().retained_event_bytes();
    let mut machine = InlineMachine::new(consumers.inline);
    loop {
        if machine.poll(InlineWork::uniform(4096)).status == InlineStatus::Ready {
            break;
        }
    }
    let retained = machine.retention();
    let whole_lower_bound = source.buffer_retention().retained_buffer_bytes
        + leaf.retained_descriptor_bytes().total()
        + lexical_retained.total()
        + retained.peak_bytes;
    eprintln!(
        "10MiB matched receipt whole_lower_bound={whole_lower_bound} machine={retained:?} lexer={lexer_receipt:?} work={:?}",
        machine.total_work(),
    );
    assert_eq!(retained.output_spans, repeats);
    assert_eq!(retained.output_payload_bytes, repeats * 4);
    assert_eq!(retained.temporary_overlay_bytes, 0);
    assert!(retained.peak_bytes > retained.bytes);
    assert!(whole_lower_bound < WHOLE_LOWER_BOUND_GATE);
}

#[test]
#[ignore = "10 MiB release-mode steady parser RSS receipt"]
fn ten_mib_dense_matched_parser_steady_rss_receipt() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    let repeats = TEN_MIB / 4;
    let (source, leaf, consumers, lexer_receipt) = lex_owned("*a* ".repeat(repeats));
    let lexical_retained = consumers.inline.view().retained_event_bytes();
    let mut machine = InlineMachine::new(consumers.inline);
    loop {
        if machine.poll(InlineWork::uniform(4096)).status == InlineStatus::Ready {
            break;
        }
    }
    let retained = machine.retention();
    let whole_lower_bound = source.buffer_retention().retained_buffer_bytes
        + leaf.retained_descriptor_bytes().total()
        + lexical_retained.total()
        + retained.peak_bytes;
    eprintln!(
        "10MiB matched steady-parser receipt whole_lower_bound={whole_lower_bound} machine={retained:?} lexer={lexer_receipt:?} work={:?}",
        machine.total_work(),
    );
    assert_eq!(retained.output_spans, repeats);
    assert_eq!(retained.output_payload_bytes, repeats * 4);
    assert_eq!(retained.temporary_overlay_bytes, 0);
}
