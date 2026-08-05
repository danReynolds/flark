use comrak::block_spine_facade as facade;
use generated_scanner_gate::{
    FusedAtxLineScanError, FusedAtxLineScanReceipt, FusedAtxLineScanResult, FusedAtxLineScanner,
    FusedAtxLineSource, CURSOR_ATX_MAX_LOOKAHEAD_SLACK, CURSOR_ATX_REJECTION_PREFIX_CAP,
    FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FullIdentity {
    revision: u64,
    root: u64,
    build: u64,
    line: u64,
    start: usize,
    end: usize,
}

impl FullIdentity {
    const fn first(len: usize) -> Self {
        Self {
            revision: 17,
            root: 29,
            build: 41,
            line: 0,
            start: 0,
            end: len,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StrictSourceError {
    BudgetContractViolated,
    NonSequential { requested: usize, expected: usize },
}

#[derive(Clone, Debug)]
struct StrictSequentialSource {
    identity: FullIdentity,
    bytes: Vec<u8>,
    next: usize,
    remaining_budget: usize,
    first_reads: usize,
}

impl StrictSequentialSource {
    fn new(identity: FullIdentity, bytes: Vec<u8>) -> Self {
        Self {
            identity,
            bytes,
            next: 0,
            remaining_budget: usize::MAX,
            first_reads: 0,
        }
    }

    fn set_budget(&mut self, budget: usize) {
        self.remaining_budget = budget;
    }
}

impl FusedAtxLineSource for StrictSequentialSource {
    type Identity = FullIdentity;
    type Error = StrictSourceError;

    fn identity(&self) -> Self::Identity {
        self.identity
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn access_budget(&self) -> usize {
        self.remaining_budget
    }

    fn read_byte(&mut self, absolute_offset: usize) -> Result<u8, Self::Error> {
        if self.remaining_budget == 0 {
            return Err(StrictSourceError::BudgetContractViolated);
        }
        if absolute_offset != self.next {
            return Err(StrictSourceError::NonSequential {
                requested: absolute_offset,
                expected: self.next,
            });
        }
        let byte = self.bytes[absolute_offset];
        self.next += 1;
        self.first_reads += 1;
        self.remaining_budget -= 1;
        Ok(byte)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RunTotals {
    polls: usize,
    lexical_work_units: usize,
    source_first_reads: usize,
    maximum_retained_source_bytes: usize,
    result: FusedAtxLineScanResult,
}

fn assert_receipt_bounds(
    receipt: FusedAtxLineScanReceipt,
    scanner: &FusedAtxLineScanner<FullIdentity>,
    source: &StrictSequentialSource,
) {
    assert!(receipt.lexical_work_units <= FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL);
    assert!(receipt.source_first_reads <= receipt.lexical_work_units);
    assert_eq!(receipt.physical_high_water, scanner.physical_high_water());
    assert_eq!(source.next, scanner.physical_high_water());
    assert_eq!(
        receipt.retained_source_bytes,
        scanner.retained_source_bytes()
    );
    assert_eq!(
        receipt.rejection_prefix_bytes,
        scanner.rejection_prefix().len()
    );
    assert_eq!(receipt.maximum_source_request_rewind_bytes, 0);
}

fn run_with_schedule(
    input: &[u8],
    mut next_fuel: impl FnMut(usize) -> usize,
    mut next_budget: impl FnMut(usize, usize) -> usize,
) -> (
    RunTotals,
    FusedAtxLineScanner<FullIdentity>,
    StrictSequentialSource,
) {
    let identity = FullIdentity::first(input.len());
    let mut source = StrictSequentialSource::new(identity, input.to_vec());
    let mut scanner = FusedAtxLineScanner::new(identity, input.len());
    let mut polls = 0;
    let mut lexical_work_units = 0;
    let mut source_first_reads = 0;
    let mut maximum_retained_source_bytes = 0;
    loop {
        let fuel = next_fuel(polls).max(1);
        source.set_budget(next_budget(polls, fuel));
        let receipt = scanner
            .poll(&mut source, fuel)
            .expect("fused ATX lexical poll");
        assert_receipt_bounds(receipt, &scanner, &source);
        polls += 1;
        lexical_work_units += receipt.lexical_work_units;
        source_first_reads += receipt.source_first_reads;
        maximum_retained_source_bytes =
            maximum_retained_source_bytes.max(receipt.retained_source_bytes);
        if receipt.result != FusedAtxLineScanResult::NeedMore {
            return (
                RunTotals {
                    polls,
                    lexical_work_units,
                    source_first_reads,
                    maximum_retained_source_bytes,
                    result: receipt.result,
                },
                scanner,
                source,
            );
        }
        assert!(polls < input.len().saturating_mul(3).saturating_add(64));
    }
}

fn physical_content_end(input: &str) -> usize {
    if input.ends_with("\r\n") {
        input.len() - 2
    } else if input
        .as_bytes()
        .last()
        .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
    {
        input.len() - 1
    } else {
        input.len()
    }
}

fn assert_donor_equivalent(input: &str, actual: FusedAtxLineScanResult) {
    let opener = facade::atx_heading_start(input).expect("pinned ATX donor facade");
    match (opener, actual) {
        (None, FusedAtxLineScanResult::NoMatch) => {}
        (Some(opener_end), FusedAtxLineScanResult::Matched(cuts)) => {
            let (chopped, closed) =
                facade::chop_trailing_hashes(input).expect("pinned ATX tail donor facade");
            let content_end = physical_content_end(input);
            assert_eq!(cuts.opener_end(), opener_end, "input={input:?}");
            assert_eq!(cuts.marker_end(), opener_end.min(content_end));
            assert_eq!(cuts.donor_chopped_end(), chopped.len());
            assert_eq!(cuts.visible_end(), chopped.len().max(cuts.marker_end()));
            assert_eq!(cuts.content_end(), content_end);
            assert_eq!(cuts.line_end(), input.len());
            assert_eq!(cuts.closed(), closed);
        }
        (expected, actual) => panic!("donor mismatch input={input:?}: {expected:?} != {actual:?}"),
    }
}

fn expect_matched(actual: FusedAtxLineScanResult) -> generated_scanner_gate::AtxLineCuts {
    let FusedAtxLineScanResult::Matched(cuts) = actual else {
        panic!("expected fused ATX match, got {actual:?}");
    };
    cuts
}

fn assert_exact_cuts(
    actual: FusedAtxLineScanResult,
    opener_end: usize,
    chopped_end: usize,
    content_end: usize,
    line_end: usize,
    closed: bool,
) {
    let cuts = expect_matched(actual);
    assert_eq!(cuts.opener_end(), opener_end);
    assert_eq!(cuts.marker_end(), opener_end.min(content_end));
    assert_eq!(cuts.donor_chopped_end(), chopped_end);
    assert_eq!(
        cuts.visible_end(),
        chopped_end.max(opener_end.min(content_end))
    );
    assert_eq!(cuts.content_end(), content_end);
    assert_eq!(cuts.line_end(), line_end);
    assert_eq!(cuts.closed(), closed);
}

#[test]
fn fused_cursor_matches_donor_at_tiny_and_random_fuel_without_source_rewind() {
    let cases = [
        "",
        "#",
        "#\n",
        "######\tβ",
        "####### text",
        "#no-space",
        "not a heading",
        "# alpha ###   \r\n",
        "# alpha#   \n",
        "###     ###\n",
        "# 😀 ##",
    ];
    for input in cases {
        for fuel in [1, 2, 7, 4_090] {
            let grant = fuel.min(4_090) + CURSOR_ATX_MAX_LOOKAHEAD_SLACK;
            let (run, scanner, source) =
                run_with_schedule(input.as_bytes(), |_| fuel, |_, _| grant);
            assert_donor_equivalent(input, run.result);
            assert_eq!(source.first_reads, run.source_first_reads);
            assert_eq!(source.next, scanner.physical_high_water());
            assert!(run.maximum_retained_source_bytes <= CURSOR_ATX_REJECTION_PREFIX_CAP + 1);
        }
    }

    let alphabet = ['#', ' ', '\t', 'a', 'Z', '\0', 'β', '😀'];
    let mut input_rng = Lcg(0x6675_7365_645f_6174);
    let mut fuel_rng = Lcg(0x785f_6675_656c_5f31);
    for case in 0..10_000 {
        let mut input = String::new();
        let len = input_rng.usize(96);
        for _ in 0..len {
            input.push(alphabet[input_rng.usize(alphabet.len())]);
        }
        match case % 4 {
            0 => input.push('\n'),
            1 => input.push_str("\r\n"),
            2 => input.push('\r'),
            _ => {}
        }
        let (run, scanner, source) = run_with_schedule(
            input.as_bytes(),
            |_| fuel_rng.usize(31) + 1,
            |_, fuel| fuel.min(4_090) + CURSOR_ATX_MAX_LOOKAHEAD_SLACK,
        );
        assert_donor_equivalent(&input, run.result);
        assert_eq!(source.next, scanner.physical_high_water());
    }
}

#[test]
fn source_budget_below_generated_slack_yields_without_touching_or_poisoning_state() {
    let input = format!("# {}x", " ".repeat(256));
    let identity = FullIdentity::first(input.len());
    let mut source = StrictSequentialSource::new(identity, input.as_bytes().to_vec());
    let mut scanner = FusedAtxLineScanner::new(identity, input.len());

    source.set_budget(CURSOR_ATX_MAX_LOOKAHEAD_SLACK);
    let stalled = scanner.poll(&mut source, 1).unwrap();
    assert_eq!(stalled.result, FusedAtxLineScanResult::NeedMore);
    assert!(stalled.source_budget_exhausted);
    assert_eq!(stalled.lexical_work_units, 0);
    assert_eq!(scanner.physical_high_water(), 0);
    assert_eq!(source.next, 0);

    // A lexical fuel of one requires a fixed generated slack grant from the
    // actor adapter. Total work remains at most 1 + YYMAXFILL - 1.
    loop {
        source.set_budget(1 + CURSOR_ATX_MAX_LOOKAHEAD_SLACK);
        let receipt = scanner.poll(&mut source, 1).unwrap();
        assert!(receipt.lexical_work_units <= 1 + CURSOR_ATX_MAX_LOOKAHEAD_SLACK);
        if receipt.result != FusedAtxLineScanResult::NeedMore {
            assert_donor_equivalent(&input, receipt.result);
            break;
        }
    }
}

#[test]
fn tail_budget_exhaustion_reports_the_consumed_constraint_only() {
    let input = format!("# x{}", "a".repeat(128));
    let identity = FullIdentity::first(input.len());
    let mut source = StrictSequentialSource::new(identity, input.as_bytes().to_vec());
    let mut scanner = FusedAtxLineScanner::new(identity, input.len());

    source.set_budget(FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL);
    let opener = scanner.poll(&mut source, 1).unwrap();
    assert_eq!(opener.result, FusedAtxLineScanResult::NeedMore);
    assert_eq!(opener.opener_logical_cut, Some(2));

    source.set_budget(1);
    let budget_bound = scanner.poll(&mut source, 17).unwrap();
    assert_eq!(budget_bound.result, FusedAtxLineScanResult::NeedMore);
    assert_eq!(budget_bound.source_first_reads, 1);
    assert!(budget_bound.source_budget_exhausted);

    source.set_budget(17);
    let fuel_bound = scanner.poll(&mut source, 1).unwrap();
    assert_eq!(fuel_bound.result, FusedAtxLineScanResult::NeedMore);
    assert_eq!(fuel_bound.source_first_reads, 1);
    assert!(!fuel_bound.source_budget_exhausted);

    loop {
        source.set_budget(31);
        let receipt = scanner.poll(&mut source, 31).unwrap();
        if receipt.result != FusedAtxLineScanResult::NeedMore {
            assert_eq!(receipt.opener_logical_cut, Some(2));
            assert!(!receipt.source_budget_exhausted);
            assert_donor_equivalent(&input, receipt.result);
            break;
        }
    }
}

#[test]
fn clone_resumes_exactly_and_full_identity_rejects_equal_length_crossing() {
    let input = format!("# {}x ###   \r\n", " ".repeat(64 * 1024));
    let identity = FullIdentity::first(input.len());
    let mut source = StrictSequentialSource::new(identity, input.as_bytes().to_vec());
    let mut scanner = FusedAtxLineScanner::new(identity, input.len());
    for _ in 0..12 {
        source.set_budget(23);
        assert_eq!(
            scanner.poll(&mut source, 17).unwrap().result,
            FusedAtxLineScanResult::NeedMore
        );
    }
    let mut resumed_scanner = scanner.clone();
    let mut resumed_source = source.clone();

    let left = loop {
        source.set_budget(37);
        let receipt = scanner.poll(&mut source, 31).unwrap();
        if receipt.result != FusedAtxLineScanResult::NeedMore {
            break receipt.result;
        }
    };
    let right = loop {
        resumed_source.set_budget(1_019);
        let receipt = resumed_scanner.poll(&mut resumed_source, 1_013).unwrap();
        if receipt.result != FusedAtxLineScanResult::NeedMore {
            break receipt.result;
        }
    };
    assert_eq!(left, right);
    assert_eq!(
        scanner.physical_high_water(),
        resumed_scanner.physical_high_water()
    );
    let opener_end = 2 + 64 * 1024;
    assert_exact_cuts(
        left,
        opener_end,
        opener_end + 1,
        input.len() - 2,
        input.len(),
        true,
    );

    let crossed_identity = FullIdentity {
        line: 1,
        start: input.len(),
        end: input.len() * 2,
        ..identity
    };
    let mut crossed = StrictSequentialSource::new(crossed_identity, input.as_bytes().to_vec());
    let mut bound = FusedAtxLineScanner::new(identity, input.len());
    assert_eq!(
        bound.poll(&mut crossed, 32),
        Err(FusedAtxLineScanError::WrongSource)
    );
    assert_eq!(crossed.next, 0);
    assert_eq!(
        bound.poll(&mut crossed, 32),
        Err(FusedAtxLineScanError::PollAfterFailure)
    );
}

#[test]
fn giant_accepted_close_and_nonclosing_tail_are_exact_and_bounded() {
    const BODY_BYTES: usize = 10 * 1024 * 1024;
    for (suffix, expected_closed) in [(" ###   \r\n", true), ("#   \n", false)] {
        let mut input = String::with_capacity(BODY_BYTES + suffix.len() + 2);
        input.push_str("# ");
        input.push_str(&"a".repeat(BODY_BYTES));
        input.push_str(suffix);
        let (run, scanner, source) = run_with_schedule(
            input.as_bytes(),
            |_| 4_090,
            |_, _| FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL,
        );
        let body_end = 2 + BODY_BYTES;
        let (chopped_end, content_end) = if expected_closed {
            (body_end, input.len() - 2)
        } else {
            (body_end + 1, input.len() - 1)
        };
        assert_exact_cuts(
            run.result,
            2,
            chopped_end,
            content_end,
            input.len(),
            expected_closed,
        );
        assert_eq!(scanner.physical_high_water(), input.len());
        assert_eq!(source.first_reads, input.len());
        assert_eq!(run.source_first_reads, input.len());
        assert!(run.polls > 2_500);
        assert!(run.maximum_retained_source_bytes <= CURSOR_ATX_REJECTION_PREFIX_CAP + 1);
    }
}

#[test]
fn giant_separator_run_streams_once_and_keeps_accepted_cut_distinct_from_high_water() {
    const SPACES: usize = 10 * 1024 * 1024;
    let mut input = String::with_capacity(SPACES + 2);
    input.push('#');
    input.push_str(&" ".repeat(SPACES));
    input.push('x');
    let identity = FullIdentity::first(input.len());
    let mut source = StrictSequentialSource::new(identity, input.as_bytes().to_vec());
    let mut scanner = FusedAtxLineScanner::new(identity, input.len());
    let mut polls = 0;
    let mut saw_opener_cut = false;
    let terminal = loop {
        source.set_budget(FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL);
        let receipt = scanner.poll(&mut source, 4_090).unwrap();
        assert_receipt_bounds(receipt, &scanner, &source);
        polls += 1;
        if let Some(opener_end) = receipt.opener_logical_cut {
            saw_opener_cut = true;
            assert_eq!(opener_end, input.len() - 1);
            assert_eq!(receipt.physical_high_water, input.len());
            assert!(receipt.physical_high_water > opener_end);
        }
        if receipt.result != FusedAtxLineScanResult::NeedMore {
            break receipt.result;
        }
    };
    assert!(saw_opener_cut);
    assert!(polls > 2_500);
    assert_eq!(source.first_reads, input.len());
    assert_exact_cuts(
        terminal,
        input.len() - 1,
        input.len(),
        input.len(),
        input.len(),
        false,
    );
}

#[test]
fn no_match_reads_only_the_generated_fixed_rejection_prefix_even_for_giant_suffixes() {
    const GIANT: usize = 10 * 1024 * 1024;
    for (prefix, expected_reads) in [("#######", 7), ("#x", 2), ("x", 1)] {
        let mut input = String::with_capacity(prefix.len() + GIANT);
        input.push_str(prefix);
        input.push_str(&" ".repeat(GIANT));
        let identity = FullIdentity::first(input.len());
        let mut source = StrictSequentialSource::new(identity, input.as_bytes().to_vec());
        source.set_budget(FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL);
        let mut scanner = FusedAtxLineScanner::new(identity, input.len());
        let receipt = scanner.poll(&mut source, 4_090).unwrap();
        assert_eq!(receipt.result, FusedAtxLineScanResult::NoMatch);
        assert_eq!(receipt.physical_high_water, expected_reads);
        assert_eq!(source.first_reads, expected_reads);
        assert_eq!(
            scanner.rejection_prefix(),
            &input.as_bytes()[..expected_reads]
        );
        assert!(expected_reads <= CURSOR_ATX_REJECTION_PREFIX_CAP);
        assert_eq!(receipt.maximum_source_request_rewind_bytes, 0);
        assert_eq!(
            scanner.poll(&mut source, 4_090),
            Err(FusedAtxLineScanError::PollAfterComplete)
        );
    }
}

fn run_block_prefix(
    input: &str,
    initial_column: usize,
    allow_initial_bom: bool,
) -> (FusedAtxLineScanResult, FusedAtxLineScanner<FullIdentity>) {
    let identity = FullIdentity::first(input.len());
    let mut source = StrictSequentialSource::new(identity, input.as_bytes().to_vec());
    let mut scanner = FusedAtxLineScanner::new_with_block_prefix(
        identity,
        input.len(),
        initial_column,
        allow_initial_bom,
    );
    loop {
        source.set_budget(FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL);
        let receipt = scanner.poll(&mut source, 1).unwrap();
        assert_receipt_bounds(receipt, &scanner, &source);
        if receipt.result != FusedAtxLineScanResult::NeedMore {
            return (receipt.result, scanner);
        }
    }
}

#[test]
fn block_prefix_owns_bom_space_and_partial_tab_column_semantics() {
    for (input, initial_column, allow_bom, start, start_column, indent, claim) in [
        ("# x\n", 0, false, 0, 0, 0, 0),
        ("   # x\n", 0, false, 3, 3, 3, 0),
        ("\t# x\n", 2, false, 1, 4, 2, 0),
        (" \t# a\0β😀\n", 1, false, 2, 4, 3, 0),
        ("\u{feff}   # x\n", 0, true, 6, 3, 3, 3),
    ] {
        let (result, scanner) = run_block_prefix(input, initial_column, allow_bom);
        assert!(matches!(result, FusedAtxLineScanResult::Matched(_)));
        let donor = scanner.donor_match().unwrap();
        assert_eq!(donor.opener_start(), start, "input={input:?}");
        assert_eq!(donor.opener_start_column(), start_column);
        assert_eq!(donor.indent_columns(), indent);
        assert_eq!(donor.claim_start(), claim);
        assert_eq!(donor.level(), 1);
    }

    for (input, initial_column, allow_bom) in [
        ("    # x\n", 0, false),
        ("\t# x\n", 0, false),
        (" \t# x\n", 0, false),
        ("\u{feff}# later\n", 0, false),
    ] {
        assert_eq!(
            run_block_prefix(input, initial_column, allow_bom).0,
            FusedAtxLineScanResult::NoMatch,
            "input={input:?}"
        );
    }
}

struct Lcg(u64);

impl Lcg {
    fn usize(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 32) as usize) % upper
    }
}
