//! Manual scaling receipt for the production M1.1 candidate components.
//!
//! This is ignored in the ordinary lane because it intentionally includes a
//! 10 MiB source. Run it in debug and release with `--ignored --nocapture`.

use std::time::{Duration, Instant};

use flark_engine::m11_host::M11_CANDIDATE_ARENA_MAX_SLOTS;
use flark_engine::parser_internal::{M11CandidatePublication, M11_MAX_ROLE_RECORDS};
use flark_engine::{
    ArenaLimits, DocumentRuntime, DocumentRuntimeConfig, ParserProfileId, RuntimeSourceFactsPoll,
    SourceFactsRootLimits, SourceFactsScanProfile, SourceStore, SOURCE_CURSOR_WINDOW_BYTES,
};
use flark_parser::{
    M11CleanDocumentKind, M11CleanDocumentResult, M11CleanParseJob, M11CleanParsePoll,
    M11ParserCandidate, M11ParserCandidateWriterPoll,
};

const DOCUMENT: [u8; 16] = [0x41; 16];
const PUBLICATION: [u8; 16] = [0x51; 16];
const CHECKPOINT_SPACING: usize = 4 * 1024;
const SOURCE_POLL_BYTES: usize = 64 * 1024;
const SOURCE_POLL_CHECKPOINTS: usize = 64;
const CANDIDATE_POLL_TRANSITIONS: usize = 32;

struct CertificationReceipt {
    polls: u64,
    elapsed: Duration,
}

struct ParseJobReceipt {
    result: M11CleanDocumentResult,
    polls: usize,
    transitions: usize,
}

fn parse_job(text: &str, fuel: usize) -> ParseJobReceipt {
    let store = SourceStore::new(text).expect("source store");
    let mut parse = M11CleanParseJob::new(store.snapshot()).expect("clean parse job");
    let mut polls = 0_usize;
    let mut transitions = 0_usize;
    loop {
        polls += 1;
        match parse.poll(fuel).expect("clean parse poll") {
            M11CleanParsePoll::Pending {
                transitions: consumed,
            } => {
                assert_eq!(consumed, fuel, "pending polls spend their exact grant");
                transitions += consumed;
            }
            M11CleanParsePoll::Complete {
                transitions: consumed,
                result,
            } => {
                assert!((1..=fuel).contains(&consumed));
                transitions += consumed;
                return ParseJobReceipt {
                    result,
                    polls,
                    transitions,
                };
            }
        }
    }
}

fn certify(runtime: &mut DocumentRuntime) -> CertificationReceipt {
    let profile = SourceFactsScanProfile::new(CHECKPOINT_SPACING).expect("scan profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let source = runtime
        .begin_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("begin runtime source facts");
    let started = Instant::now();
    let mut polls = 0_u64;
    loop {
        polls += 1;
        match runtime
            .poll_source_facts(SOURCE_POLL_BYTES, SOURCE_POLL_CHECKPOINTS)
            .expect("runtime source facts poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
            RuntimeSourceFactsPoll::Complete { completion, .. } => {
                assert_eq!(completion.source(), source);
                return CertificationReceipt {
                    polls,
                    elapsed: started.elapsed(),
                };
            }
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean SourceFacts scan reported incremental progress")
            }
        }
    }
}

#[test]
fn one_mib_single_line_uses_window_bounded_candidate_transitions() {
    const SOURCE_BYTES: usize = 1024 * 1024;
    let source = "x".repeat(SOURCE_BYTES);
    let store = SourceStore::new(&source).expect("source store");
    let mut parse = M11CleanParseJob::new(store.snapshot()).expect("clean parse job");
    let mut polls = 0_usize;
    let mut transitions = 0_usize;
    let expected_end = u32::try_from(SOURCE_BYTES).expect("M1.1 source range");
    let result = loop {
        polls += 1;
        match parse
            .poll(CANDIDATE_POLL_TRANSITIONS)
            .expect("clean parse poll")
        {
            M11CleanParsePoll::Pending {
                transitions: consumed,
            } => transitions += consumed,
            M11CleanParsePoll::Complete {
                transitions: consumed,
                result,
            } => {
                transitions += consumed;
                break result;
            }
        }
    };

    assert_eq!(result.kind(), M11CleanDocumentKind::Paragraph);
    assert_eq!(result.source_range(), 0..expected_end);
    assert_eq!(result.visible_source(), Some(0..expected_end));
    assert!(result.definitions().is_empty());
    let source_windows = SOURCE_BYTES.div_ceil(SOURCE_CURSOR_WINDOW_BYTES);
    let maximum_transitions = source_windows * 2 + 2;
    assert!(
        transitions <= maximum_transitions,
        "line discovery plus segmented lexical input must remain window-granular: {transitions} > {maximum_transitions}"
    );
    assert!(
        polls <= maximum_transitions.div_ceil(CANDIDATE_POLL_TRANSITIONS),
        "endpoint-shaped fuel must not need one scheduled poll per byte: {polls}"
    );
}

#[test]
fn aggregate_quantum_is_shape_bounded_and_fuel_partition_invariant() {
    let cr_at_discovery_boundary = format!(
        "{}\ry",
        "x".repeat(SOURCE_CURSOR_WINDOW_BYTES.saturating_sub(2))
    );
    let cases = [
        ("empty", String::new(), 1_usize),
        ("giant", "x".repeat(256 * 1024), 1),
        (
            "80-byte-lines",
            representative_lines(256 * 1024),
            (256_usize * 1024).div_ceil(80),
        ),
        ("newline-only", "\n".repeat(64 * 1024), 64 * 1024),
        ("crlf-unicode", "é😀word\r\n".repeat(4 * 1024), 4 * 1024),
        ("cr-at-discovery-boundary", cr_at_discovery_boundary, 2),
    ];

    for (shape, source, physical_lines) in cases {
        let fuel_one = parse_job(&source, 1);
        let fuel_32 = parse_job(&source, CANDIDATE_POLL_TRANSITIONS);
        // Each run owns a different immutable source root, so the opaque
        // parser capabilities must not compare equal across runs. Fuel
        // partition invariance applies to their parser-derived semantics.
        assert_eq!(fuel_one.result.kind(), fuel_32.result.kind(), "{shape}");
        assert_eq!(
            fuel_one.result.source_range(),
            fuel_32.result.source_range(),
            "{shape}"
        );
        assert_eq!(
            fuel_one.result.visible_source(),
            fuel_32.result.visible_source(),
            "{shape}"
        );
        assert_eq!(
            fuel_one.result.definitions(),
            fuel_32.result.definitions(),
            "{shape}"
        );
        assert_eq!(fuel_one.transitions, fuel_32.transitions, "{shape}");
        println!(
            "m11_quantum_shape shape={shape} bytes={} lines={physical_lines} transitions={} fuel1_polls={} fuel32_polls={}",
            source.len(),
            fuel_one.transitions,
            fuel_one.polls,
            fuel_32.polls,
        );
        assert_eq!(fuel_one.polls, fuel_one.transitions, "{shape}");
        assert_eq!(
            fuel_32.polls,
            fuel_32.transitions.div_ceil(CANDIDATE_POLL_TRANSITIONS),
            "{shape}"
        );

        // Each byte may be inspected once for physical-line discovery and
        // once by the segmented controller. A deliberately loose per-line
        // envelope covers classifier plus explicit line/boundary state work
        // while still detecting any regression to one transition per phase.
        let maximum_accounted_work = source
            .len()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(physical_lines * 32 + 1))
            .expect("fixture work envelope");
        let maximum_transitions = maximum_accounted_work.div_ceil(SOURCE_CURSOR_WINDOW_BYTES) + 1;
        assert!(
            fuel_one.transitions <= maximum_transitions,
            "{shape} used {} aggregate transitions above {maximum_transitions}",
            fuel_one.transitions
        );

        let minimum_accounted_work = source
            .len()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(physical_lines * 3 + 1))
            .expect("fixture minimum work envelope");
        let minimum_transitions = minimum_accounted_work.div_ceil(SOURCE_CURSOR_WINDOW_BYTES);
        assert!(
            fuel_one.transitions >= minimum_transitions,
            "{shape} hid discovery, lexical, line-boundary, or finish work: {} < {minimum_transitions}",
            fuel_one.transitions
        );
    }
}

#[test]
#[ignore = "manual production-shape 128 B / 1 MiB / 10 MiB scaling receipt"]
fn giant_single_line_candidate_scaling_receipt() {
    for bytes in [128_usize, 1024 * 1024, 10 * 1024 * 1024] {
        run_candidate_case("single_line", &"x".repeat(bytes));
    }
}

#[test]
#[ignore = "manual production-shape 1 MiB / 10 MiB representative-line scaling receipt"]
fn representative_line_candidate_scaling_receipt() {
    for bytes in [1024 * 1024, 10 * 1024 * 1024] {
        run_candidate_case("80_byte_lines", &representative_lines(bytes));
    }
}

#[test]
#[ignore = "manual production-shape 1 MiB / 10 MiB newline-dense scaling receipt"]
fn newline_dense_candidate_scaling_receipt() {
    for bytes in [1024 * 1024, 10 * 1024 * 1024] {
        let source = "x\n".repeat(bytes / 2);
        assert_eq!(source.len(), bytes);
        run_candidate_case("2_byte_lines", &source);
    }
}

fn representative_lines(bytes: usize) -> String {
    const PHYSICAL_LINE_BYTES: usize = 80;
    let mut source = String::with_capacity(bytes);
    let complete_lines = bytes / PHYSICAL_LINE_BYTES;
    let remainder = bytes % PHYSICAL_LINE_BYTES;
    let line = format!("{}\n", "x".repeat(PHYSICAL_LINE_BYTES - 1));
    for _ in 0..complete_lines {
        source.push_str(&line);
    }
    source.extend(std::iter::repeat_n('x', remainder));
    assert_eq!(source.len(), bytes);
    source
}

fn close_candidate_case(mut runtime: DocumentRuntime, mut publication: M11CandidatePublication) {
    publication
        .begin_close(&mut runtime)
        .expect("begin publication close");
    while !publication
        .poll_close(&mut runtime, CANDIDATE_POLL_TRANSITIONS)
        .expect("publication close poll")
    {}
    runtime.begin_close().expect("begin runtime close");
    while !runtime
        .poll_close(CANDIDATE_POLL_TRANSITIONS)
        .expect("runtime close poll")
        .complete
    {}
}

fn run_candidate_case(shape: &str, source: &str) {
    let bytes = source.len();
    let mut runtime = DocumentRuntime::new(
        source,
        DocumentRuntimeConfig {
            arena_limits: ArenaLimits {
                max_slots: M11_CANDIDATE_ARENA_MAX_SLOTS,
                max_live_payload_bytes: 64 * 1024 * 1024,
                max_children_per_node: M11_MAX_ROLE_RECORDS,
            },
            ..DocumentRuntimeConfig::default()
        },
    )
    .expect("producer runtime");
    let certification = certify(&mut runtime);

    let parse_started = Instant::now();
    let parse_lease = runtime
        .certified_source()
        .expect("completed source facts")
        .exact_parse_lease();
    let mut parse = M11CleanParseJob::new(parse_lease).expect("clean parse job");
    let mut parse_polls = 0_u64;
    let mut parse_transitions = 0_u64;
    let result = loop {
        parse_polls += 1;
        match parse
            .poll(CANDIDATE_POLL_TRANSITIONS)
            .expect("clean parse poll")
        {
            M11CleanParsePoll::Pending { transitions } => {
                parse_transitions += u64::try_from(transitions).expect("transition count");
            }
            M11CleanParsePoll::Complete {
                transitions,
                result,
            } => {
                parse_transitions += u64::try_from(transitions).expect("transition count");
                break result;
            }
        }
    };
    let parse_elapsed = parse_started.elapsed();
    let expected_end = u32::try_from(bytes).expect("M1.1 source range");
    assert_eq!(result.kind(), M11CleanDocumentKind::Paragraph);
    assert_eq!(result.source_range(), 0..expected_end);
    assert_eq!(result.visible_source(), Some(0..expected_end));
    assert!(result.definitions().is_empty());

    let derive_started = Instant::now();
    let certified = runtime
        .take_certified_source()
        .expect("runtime certification");
    let candidate =
        M11ParserCandidate::derive_segmented(certified, result).expect("segmented candidate");
    let derive_elapsed = derive_started.elapsed();

    let writer_started = Instant::now();
    let mut writer = candidate
        .into_writer(&mut runtime, DOCUMENT, PUBLICATION, 1)
        .expect("candidate writer");
    let mut writer_polls = 0_u64;
    let mut writer_transitions = 0_u64;
    let publication = loop {
        writer_polls += 1;
        match writer
            .poll(&mut runtime, CANDIDATE_POLL_TRANSITIONS)
            .expect("candidate writer poll")
        {
            M11ParserCandidateWriterPoll::Pending { transitions } => {
                writer_transitions += u64::try_from(transitions).expect("transition count");
            }
            M11ParserCandidateWriterPoll::Published {
                transitions,
                publication,
            } => {
                writer_transitions += u64::try_from(transitions).expect("transition count");
                break publication;
            }
        }
    };
    let writer_elapsed = writer_started.elapsed();
    let descriptor = publication
        .descriptor(&runtime)
        .expect("publication descriptor");

    println!(
        "m11_large_candidate shape={shape} bytes={bytes} certification_polls={} certification_ms={:.3} parse_polls={parse_polls} parse_transitions={parse_transitions} parse_ms={:.3} derive_ms={:.3} writer_polls={writer_polls} writer_transitions={writer_transitions} writer_ms={:.3} terminal=published records={}",
        certification.polls,
        certification.elapsed.as_secs_f64() * 1000.0,
        parse_elapsed.as_secs_f64() * 1000.0,
        derive_elapsed.as_secs_f64() * 1000.0,
        writer_elapsed.as_secs_f64() * 1000.0,
        descriptor.canonical_record_count,
    );
    close_candidate_case(runtime, *publication);
}
