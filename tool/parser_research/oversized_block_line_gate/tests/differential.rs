use comrak::block_spine_facade::{self as facade, FacadeAlignment, FacadeSetextChar};
use flark_oversized_block_line_gate::{
    AtxStartJob, AtxTailJob, CancellationToken, ChoppedAtx, FenceJob, FenceMode, HtmlEndJob,
    HtmlType7Job, MAX_TABLE_CELLS, MarkerLineJob, MarkerLineResult, Poll, ReferenceDefinitionShape,
    ReferencePrefixJob, StreamingTableRowJob, TableRowJob, TableRowStreamPoll, TableRowSummary,
};

fn complete<T>(mut poll: impl FnMut() -> Poll<T>) -> T {
    loop {
        match poll() {
            Poll::Pending { .. } => {}
            Poll::Ready { value, .. } => return value,
            Poll::Cancelled { .. } => panic!("unexpected cancellation"),
        }
    }
}

fn fence(input: &str, mode: FenceMode, fuel: usize) -> Option<usize> {
    let mut job = FenceJob::new(input.as_bytes(), mode);
    complete(|| job.poll(input.as_bytes(), fuel, &CancellationToken::default()))
}

fn marker(input: &str, fuel: usize) -> MarkerLineResult {
    let mut job = MarkerLineJob::new(input.as_bytes());
    complete(|| job.poll(input.as_bytes(), fuel, &CancellationToken::default()))
}

fn atx_start(input: &str, fuel: usize) -> Option<usize> {
    let mut job = AtxStartJob::new(input.as_bytes());
    complete(|| job.poll(input.as_bytes(), fuel, &CancellationToken::default()))
}

fn atx_tail(input: &str, fuel: usize) -> ChoppedAtx {
    let mut job = AtxTailJob::new(input.as_bytes());
    complete(|| job.poll(input.as_bytes(), fuel, &CancellationToken::default()))
}

fn html_end(input: &str, block_type: u8, fuel: usize) -> bool {
    let mut job = HtmlEndJob::new(input.as_bytes(), block_type);
    complete(|| job.poll(input.as_bytes(), fuel, &CancellationToken::default()))
}

fn html_type7(input: &str, fuel: usize) -> bool {
    let mut job = HtmlType7Job::new(input.as_bytes());
    complete(|| job.poll(input.as_bytes(), fuel, &CancellationToken::default()))
}

fn table(input: &str, fuel: usize) -> Option<TableRowSummary> {
    let mut job = TableRowJob::new(input.as_bytes());
    complete(|| job.poll(input.as_bytes(), fuel, &CancellationToken::default()))
}

fn streaming_table(input: &str, fuel: usize) -> Option<TableRowSummary> {
    let mut job = StreamingTableRowJob::new(input.as_bytes());
    let token = CancellationToken::default();
    let mut cells = Vec::new();
    let mut alignments = Vec::new();
    loop {
        match job.poll(input.as_bytes(), fuel, &token) {
            TableRowStreamPoll::Pending { .. } => {}
            TableRowStreamPoll::Cell { value, .. } => {
                cells.push(value.cell);
                alignments.push(value.delimiter_alignment);
            }
            TableRowStreamPoll::Complete { value, .. } => {
                return value.map(|summary| {
                    assert_eq!(usize::try_from(summary.cells).ok(), Some(cells.len()));
                    let delimiter_alignments = summary.delimiter_row.then(|| {
                        alignments
                            .into_iter()
                            .map(|alignment| alignment.expect("valid delimiter cell alignment"))
                            .collect()
                    });
                    TableRowSummary {
                        cells,
                        delimiter_alignments,
                    }
                });
            }
            TableRowStreamPoll::Cancelled { .. } => panic!("unexpected cancellation"),
        }
    }
}

fn reference(input: &str, fuel: usize) -> Option<ReferenceDefinitionShape> {
    let mut job = ReferencePrefixJob::new();
    complete(|| job.poll(input.as_bytes(), fuel, &CancellationToken::default()))
}

#[test]
fn fixed_scanner_correspondents_match_facade_at_every_fuel() {
    let lines = [
        "x\n",
        "#\n",
        "###### heading ###   \r\n",
        "####### nope\n",
        "``` rust\n",
        "``` a`b\n",
        "~~~~~~~ anything ` ok\n",
        "```   \n",
        "---\n",
        "- - -\n",
        "===  \n",
        "abc --> tail\n",
        "ABC </ScRiPt> tail\n",
        "abc ]]> tail\n",
    ];
    for input in lines {
        for fuel in [1, 2, 3, 7, 4096] {
            assert_eq!(
                fence(input, FenceMode::Open, fuel),
                facade::open_code_fence(input).unwrap(),
                "open {input:?} fuel={fuel}"
            );
            assert_eq!(
                fence(input, FenceMode::Close, fuel),
                facade::close_code_fence(input).unwrap(),
                "close {input:?} fuel={fuel}"
            );
            assert_eq!(
                atx_start(input, fuel),
                facade::atx_heading_start(input).unwrap(),
                "atx {input:?} fuel={fuel}"
            );
            let expected_setext =
                facade::setext_heading_line(input)
                    .unwrap()
                    .map(|kind| match kind {
                        FacadeSetextChar::Equals => b'=',
                        FacadeSetextChar::Hyphen => b'-',
                    });
            assert_eq!(
                marker(input, fuel).setext,
                expected_setext,
                "setext {input:?} fuel={fuel}"
            );
            for block_type in 1..=5 {
                assert_eq!(
                    html_end(input, block_type, fuel),
                    facade::html_block_end(block_type, input).unwrap(),
                    "html end {block_type} {input:?} fuel={fuel}"
                );
            }
            if !input.is_empty() {
                let (expected, closed) = facade::chop_trailing_hashes(input).unwrap();
                assert_eq!(
                    atx_tail(input, fuel),
                    ChoppedAtx {
                        end: expected.len(),
                        closed
                    },
                    "tail {input:?} fuel={fuel}"
                );
            }
        }
    }
}

#[test]
fn fixed_scanner_correspondents_match_random_physical_lines() {
    let alphabet = b"`~#*-_=<>/ abcXYZ09\t";
    let mut rng = Lcg(0x051c_e5ca);
    for case in 0..20_000 {
        let len = 1 + rng.usize(160);
        let mut input: String = (0..len)
            .map(|_| alphabet[rng.usize(alphabet.len())] as char)
            .collect();
        match case % 3 {
            0 => input.push('\n'),
            1 => input.push_str("\r\n"),
            _ => {}
        }
        for fuel in [1, 4096] {
            assert_eq!(
                fence(&input, FenceMode::Open, fuel),
                facade::open_code_fence(&input).unwrap(),
                "open case={case} fuel={fuel} input={input:?}"
            );
            assert_eq!(
                fence(&input, FenceMode::Close, fuel),
                facade::close_code_fence(&input).unwrap(),
                "close case={case} fuel={fuel} input={input:?}"
            );
            assert_eq!(
                atx_start(&input, fuel),
                facade::atx_heading_start(&input).unwrap(),
                "atx case={case} fuel={fuel} input={input:?}"
            );
            let expected_setext =
                facade::setext_heading_line(&input)
                    .unwrap()
                    .map(|kind| match kind {
                        FacadeSetextChar::Equals => b'=',
                        FacadeSetextChar::Hyphen => b'-',
                    });
            assert_eq!(
                marker(&input, fuel).setext,
                expected_setext,
                "setext case={case} fuel={fuel} input={input:?}"
            );
            let tail_input = format!("a{input}");
            let (expected_tail, closed) = facade::chop_trailing_hashes(&tail_input).unwrap();
            assert_eq!(
                atx_tail(&tail_input, fuel),
                ChoppedAtx {
                    end: expected_tail.len(),
                    closed
                },
                "tail case={case} fuel={fuel} input={tail_input:?}"
            );
            for block_type in 1..=5 {
                assert_eq!(
                    html_end(&input, block_type, fuel),
                    facade::html_block_end(block_type, &input).unwrap(),
                    "html end type={block_type} case={case} fuel={fuel} input={input:?}"
                );
            }
        }
    }
}

#[test]
fn html_type7_matches_generated_donor_on_random_ascii() {
    let alphabet = b"<>/='\" abcXYZ09-_:\t\x0b\x0c`";
    let mut rng = Lcg(0x71_7e_57);
    for case in 0..30_000 {
        let len = rng.usize(96);
        let mut input: String = (0..len)
            .map(|_| alphabet[rng.usize(alphabet.len())] as char)
            .collect();
        match case % 3 {
            0 => input.push('\n'),
            1 => input.push_str("\r\n"),
            _ => {}
        }
        let earlier = facade::html_block_start(&input, false).unwrap();
        let expected =
            earlier.is_none() && facade::html_block_start(&input, true).unwrap() == Some(7);
        for fuel in [1, 7, 4096] {
            assert_eq!(
                html_type7(&input, fuel),
                expected,
                "case={case} fuel={fuel} input={input:?}"
            );
        }
    }
}

#[test]
fn table_rows_match_donor_ranges_content_and_alignments() {
    let fixed = [
        "a | b\n",
        "| a | b |\r\n",
        "||\n",
        "|  |\n",
        r"a \| b | c",
        r"a \\| b | c",
        "| :--- | ---: |\n",
        "| --- | :---: |\n",
        "not a row\n",
    ];
    for input in fixed {
        compare_table(input);
    }

    let alphabet = b"|\\:- abcXYZ09\t\x0b\x0c";
    let mut rng = Lcg(0x7a_b1e);
    for case in 0..20_000 {
        let len = 1 + rng.usize(128);
        let mut input: String = (0..len)
            .map(|_| alphabet[rng.usize(alphabet.len())] as char)
            .collect();
        input.push(if case % 2 == 0 { '\n' } else { '\r' });
        compare_table(&input);
    }
}

fn compare_table(input: &str) {
    let expected = facade::table_row(input, false).unwrap();
    for fuel in [1, 7, 4096] {
        let actual = table(input, fuel);
        assert_eq!(
            streaming_table(input, fuel),
            actual,
            "streaming compatibility fuel={fuel} input={input:?}"
        );
        assert_eq!(
            actual.is_some(),
            expected.is_some(),
            "presence fuel={fuel} input={input:?}"
        );
        if let (Some(actual), Some(expected)) = (&actual, &expected) {
            assert_eq!(
                actual.cells.len(),
                expected.cells.len(),
                "count fuel={fuel} input={input:?}"
            );
            for (left, right) in actual.cells.iter().zip(&expected.cells) {
                assert_eq!(
                    left.source, right.source,
                    "source fuel={fuel} input={input:?}"
                );
                assert_eq!(
                    left.internal_offset, right.internal_offset,
                    "offset fuel={fuel} input={input:?}"
                );
                assert_eq!(
                    left.had_escaped_pipe, right.had_escaped_pipe,
                    "escape fuel={fuel} input={input:?}"
                );
                assert_eq!(
                    materialize_cell(input, left.content.clone()),
                    right.content,
                    "content fuel={fuel} input={input:?}"
                );
            }
            let expected_alignments = facade::table_delimiter_alignments(input, false)
                .unwrap()
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| match item {
                            FacadeAlignment::None => 0,
                            FacadeAlignment::Left => 1,
                            FacadeAlignment::Center => 2,
                            FacadeAlignment::Right => 3,
                        })
                        .collect::<Vec<_>>()
                });
            assert_eq!(
                actual.delimiter_alignments, expected_alignments,
                "alignment fuel={fuel} input={input:?}"
            );
        }
    }
}

#[test]
fn streaming_table_state_is_constant_and_crosses_the_legacy_cell_cap() {
    let cell_count = MAX_TABLE_CELLS + 2;
    let input = "x|".repeat(cell_count);
    let token = CancellationToken::default();
    let mut job = StreamingTableRowJob::new(input.as_bytes());
    let accounted = job.accounted_bytes();
    let mut emitted = 0_usize;
    let mut serialized_once = false;

    loop {
        assert_eq!(job.accounted_bytes(), accounted);
        match job.poll(input.as_bytes(), 17, &token) {
            TableRowStreamPoll::Pending { inspected } => assert!(inspected <= 17),
            TableRowStreamPoll::Cell { value, inspected } => {
                assert!(inspected <= 17);
                assert_eq!(value.cell.content.len(), 1);
                assert_eq!(value.delimiter_alignment, None);
                emitted += 1;
                if emitted == 100 {
                    let encoded = serde_json::to_vec(&job).unwrap();
                    assert!(
                        encoded.len() < 2_048,
                        "checkpoint grew to {} bytes",
                        encoded.len()
                    );
                    job = serde_json::from_slice(&encoded).unwrap();
                    serialized_once = true;
                }
            }
            TableRowStreamPoll::Complete { value, inspected } => {
                assert_eq!(inspected, 0);
                let summary = value.expect("dense row remains valid");
                assert_eq!(usize::try_from(summary.cells).unwrap(), cell_count);
                assert!(!summary.delimiter_row);
                break;
            }
            TableRowStreamPoll::Cancelled { .. } => panic!("unexpected cancellation"),
        }
    }

    assert!(serialized_once);
    assert_eq!(emitted, cell_count);
    assert_eq!(job.receipt().bytes_inspected, input.len());
    assert!(job.receipt().maximum_bytes_per_poll <= 17);
    assert!(job.receipt().polls >= cell_count);
}

fn materialize_cell(input: &str, range: std::ops::Range<usize>) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(range.len());
    let mut index = range.start;
    while index < range.end {
        if bytes[index] != b'\\' {
            output.push(bytes[index]);
            index += 1;
            continue;
        }
        let run_start = index;
        while index < range.end && bytes[index] == b'\\' {
            index += 1;
        }
        let run = index - run_start;
        output.extend(std::iter::repeat_n(
            b'\\',
            run - usize::from(run % 2 == 1 && input.as_bytes().get(index) == Some(&b'|')),
        ));
        if index < range.end && bytes[index] == b'|' {
            output.push(b'|');
            index += 1;
        }
    }
    String::from_utf8(output).unwrap()
}

#[test]
fn reference_shape_matches_donor_on_fixed_and_random_ascii() {
    let fixed = [
        "[x]: /url\n",
        "[ x ]:\t<url> \"title\"\r\n",
        "[x]: a(b)c 'title'\n",
        "[x]: a\\(b\\) (title)\n",
        "[x]: <u>\n  (title)\n",
        "[x]: u\n[next]: v\n",
        "[x]: u \"bad\" junk\n",
        "[x]: u\n \"bad\" junk\n",
        "[x\\]]: /url\n",
        "[x\\[]: /url\n",
        "[]: /url\n",
        "[x]: <>\n",
        "[x]: <u>",
        "[x]: u",
    ];
    for input in fixed {
        compare_reference(input);
    }

    let alphabet = b"[]:<>/()\\'\" abcXYZ09-_\t\r\n";
    let mut rng = Lcg(0x5e_fe_12);
    for _ in 0..35_000 {
        let len = rng.usize(160);
        let input: String = (0..len)
            .map(|_| alphabet[rng.usize(alphabet.len())] as char)
            .collect();
        compare_reference(&input);
    }
}

fn compare_reference(input: &str) {
    let expected = facade::reference_definitions(input)
        .unwrap()
        .into_iter()
        .next();
    for fuel in [1, 2, 7, 4096] {
        let actual = reference(input, fuel);
        assert_eq!(
            actual.is_some(),
            expected.is_some(),
            "presence fuel={fuel} input={input:?}"
        );
        if let (Some(actual), Some(expected)) = (actual, &expected) {
            assert_eq!(
                actual.source, expected.source,
                "source fuel={fuel} input={input:?}"
            );
            assert_eq!(
                actual.label, expected.label_source,
                "label fuel={fuel} input={input:?}"
            );
            assert_eq!(
                actual.destination, expected.url_source,
                "url fuel={fuel} input={input:?}"
            );
            assert_eq!(
                actual.title, expected.title_source,
                "title fuel={fuel} input={input:?}"
            );
        }
    }
}

#[test]
fn serialized_checkpoint_and_cancellation_preserve_byte_bound() {
    let input = format!("<x a=\"{}\">\n", "v".repeat(20_000));
    let mut job = HtmlType7Job::new(input.as_bytes());
    assert!(matches!(
        job.poll(input.as_bytes(), 17, &CancellationToken::default()),
        Poll::Pending { inspected: 17 }
    ));
    let encoded = serde_json::to_vec(&job).unwrap();
    let mut resumed: HtmlType7Job = serde_json::from_slice(&encoded).unwrap();
    assert!(complete(|| resumed.poll(
        input.as_bytes(),
        17,
        &CancellationToken::default()
    )));
    assert!(resumed.receipt().maximum_bytes_per_poll <= 17);

    let token = CancellationToken::default();
    let mut table = TableRowJob::new(input.as_bytes());
    assert!(matches!(
        table.poll(input.as_bytes(), 23, &token),
        Poll::Pending { inspected: 23 }
    ));
    token.cancel();
    assert!(matches!(
        table.poll(input.as_bytes(), 23, &token),
        Poll::Cancelled { inspected: 0 }
    ));
    assert!(table.receipt().maximum_bytes_per_poll <= 23);
}

struct Lcg(u64);

impl Lcg {
    fn usize(&mut self, limit: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        ((self.0 >> 32) as usize) % limit.max(1)
    }
}
