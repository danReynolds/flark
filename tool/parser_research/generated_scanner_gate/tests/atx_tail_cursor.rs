use comrak::block_spine_facade as facade;
use generated_scanner_gate::{
    AtxLineCutsError, AtxTailCursorScanner, AtxTailCursorSource, AtxTailCuts, AtxTailScanError,
    AtxTailScanResult,
};

struct CountingSource {
    key: u64,
    bytes: Vec<u8>,
    accesses: usize,
    previous_request: Option<usize>,
    maximum_requested_rewind: usize,
}

impl CountingSource {
    fn new(key: u64, bytes: Vec<u8>) -> Self {
        Self {
            key,
            bytes,
            accesses: 0,
            previous_request: None,
            maximum_requested_rewind: 0,
        }
    }
}

impl AtxTailCursorSource for CountingSource {
    fn source_key(&self) -> u64 {
        self.key
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn byte_at(&mut self, absolute_offset: usize) -> u8 {
        if let Some(previous) = self.previous_request {
            self.maximum_requested_rewind = self
                .maximum_requested_rewind
                .max(previous.saturating_sub(absolute_offset));
        }
        self.previous_request = Some(absolute_offset);
        self.accesses += 1;
        self.bytes[absolute_offset]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RunReceipt {
    cuts: AtxTailCuts,
    cursor: usize,
    source_high_water: usize,
    source_byte_requests: usize,
    maximum_source_request_rewind: usize,
}

fn run_with_fuel(bytes: &[u8], fuel: usize) -> RunReceipt {
    let mut source = CountingSource::new(17, bytes.to_vec());
    let mut scanner = AtxTailCursorScanner::new(source.source_key(), source.len());
    let mut source_byte_requests = 0;
    loop {
        let receipt = scanner.poll(&mut source, fuel).expect("ATX tail scan");
        assert!(receipt.source_byte_requests <= fuel);
        assert_eq!(receipt.maximum_source_request_rewind_bytes, 0);
        assert_eq!(receipt.retained_source_bytes, 0);
        assert_eq!(receipt.source_high_water, scanner.source_high_water());
        source_byte_requests += receipt.source_byte_requests;
        match receipt.result {
            AtxTailScanResult::NeedMore => {}
            AtxTailScanResult::Complete(cuts) => {
                assert_eq!(source.accesses, source_byte_requests);
                return RunReceipt {
                    cuts,
                    cursor: scanner.cursor(),
                    source_high_water: scanner.source_high_water(),
                    source_byte_requests,
                    maximum_source_request_rewind: source.maximum_requested_rewind,
                };
            }
        }
    }
}

fn run_with_random_fuel(bytes: &[u8], rng: &mut Lcg) -> RunReceipt {
    let mut source = CountingSource::new(19, bytes.to_vec());
    let mut scanner = AtxTailCursorScanner::new(source.source_key(), source.len());
    let mut source_byte_requests = 0;
    loop {
        let fuel = rng.usize(31) + 1;
        let receipt = scanner
            .poll(&mut source, fuel)
            .expect("random-fuel ATX tail scan");
        assert!(receipt.source_byte_requests <= fuel);
        source_byte_requests += receipt.source_byte_requests;
        match receipt.result {
            AtxTailScanResult::NeedMore => {}
            AtxTailScanResult::Complete(cuts) => {
                return RunReceipt {
                    cuts,
                    cursor: scanner.cursor(),
                    source_high_water: scanner.source_high_water(),
                    source_byte_requests,
                    maximum_source_request_rewind: source.maximum_requested_rewind,
                };
            }
        }
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

fn expected(input: &str) -> (usize, bool) {
    let (chopped, closed) = facade::chop_trailing_hashes(input).expect("bounded donor facade");
    (chopped.len(), closed)
}

fn assert_donor_result(input: &str, actual: RunReceipt) {
    let (chopped_end, closed) = expected(input);
    assert_eq!(actual.cuts.chopped_end(), chopped_end, "input={input:?}");
    assert_eq!(actual.cuts.closed(), closed, "input={input:?}");
    assert_eq!(actual.cuts.content_end(), physical_content_end(input));
    assert_eq!(actual.cuts.line_end(), input.len());
    assert_eq!(actual.cursor, input.len());
    assert_eq!(actual.source_high_water, input.len());
    assert_eq!(actual.source_byte_requests, input.len());
    assert_eq!(actual.maximum_source_request_rewind, 0);
}

#[test]
fn atx_tail_matches_pinned_donor_at_every_tiny_fuel_and_composes_line_cuts() {
    let cases = [
        "#",
        "#\n",
        "#\r",
        "#\r\n",
        "# alpha",
        "# alpha   \n",
        "# alpha#   \n",
        "# alpha ###   \r\n",
        "# alpha\t###\r",
        "###     ###\n",
        " ###",
        "######\tβ ###\r\n",
        "# 😀 ##",
        "# alpha\u{b}",
        "# alpha\u{c}",
        "# alpha\u{a0}",
        "# alpha\u{2003}",
    ];
    for input in cases {
        for fuel in 1..=input.len() + 1 {
            let actual = run_with_fuel(input.as_bytes(), fuel);
            assert_donor_result(input, actual);
        }
    }

    for (input, first_nonspace) in [
        ("# alpha ###   \r\n", 0),
        ("#   \n", 0),
        ("###     ###\n", 0),
        ("  ###     ###\n", 2),
    ] {
        let cuts = run_with_fuel(input.as_bytes(), 1).cuts;
        let opener_end = first_nonspace
            + facade::atx_heading_start(&input[first_nonspace..])
                .expect("bounded ATX opener facade")
                .expect("ATX heading");
        let line = cuts
            .with_opener_end(opener_end)
            .expect("opener belongs to line");
        assert_eq!(line.opener_end(), opener_end);
        assert_eq!(line.marker_end(), opener_end.min(line.content_end()));
        assert_eq!(
            line.visible_end(),
            line.donor_chopped_end().max(line.marker_end())
        );
        assert!(line.marker_end() <= line.visible_end());
        assert!(line.visible_end() <= line.content_end());
        assert!(line.content_end() <= line.line_end());
        assert_eq!(line.closed(), cuts.closed());
    }
}

#[test]
fn atx_tail_matches_strong_randomized_donor_differential_and_fuel() {
    let alphabet = [
        '#', ' ', '\t', '\r', '\n', 'a', 'Z', '0', '\u{b}', '\u{c}', 'β', '😀', '\u{a0}',
        '\u{2003}',
    ];
    let mut input_rng = Lcg(0x6174_785f_7461_696c);
    let mut fuel_rng = Lcg(0x6174_785f_6675_656c);
    for case in 0..30_000 {
        let mut input = String::from("# ");
        let len = input_rng.usize(192);
        for _ in 0..len {
            input.push(alphabet[input_rng.usize(alphabet.len())]);
        }
        match case % 4 {
            0 => input.push('\n'),
            1 => input.push_str("\r\n"),
            2 => input.push('\r'),
            _ => {}
        }
        for fuel in [1, 2, 7, 4_090] {
            assert_donor_result(&input, run_with_fuel(input.as_bytes(), fuel));
        }
        assert_donor_result(
            &input,
            run_with_random_fuel(input.as_bytes(), &mut fuel_rng),
        );
    }
}

#[test]
fn atx_tail_clone_resumes_under_a_different_schedule() {
    let input = format!("# {} ###   \r\n", "β".repeat(3 * 1024));
    let mut source = CountingSource::new(29, input.as_bytes().to_vec());
    let mut scanner = AtxTailCursorScanner::new(source.source_key(), source.len());
    for _ in 0..8 {
        assert_eq!(
            scanner.poll(&mut source, 17).expect("prefix poll").result,
            AtxTailScanResult::NeedMore
        );
    }
    let mut resumed = scanner.clone();
    let mut resumed_source = CountingSource::new(29, input.as_bytes().to_vec());

    let left = loop {
        match scanner
            .poll(&mut source, 31)
            .expect("original resume")
            .result
        {
            AtxTailScanResult::NeedMore => {}
            AtxTailScanResult::Complete(cuts) => break cuts,
        }
    };
    let right = loop {
        match resumed
            .poll(&mut resumed_source, 1_013)
            .expect("clone resume")
            .result
        {
            AtxTailScanResult::NeedMore => {}
            AtxTailScanResult::Complete(cuts) => break cuts,
        }
    };
    assert_eq!(left, right);
    assert_eq!(scanner.cursor(), resumed.cursor());
    assert_eq!(scanner.source_high_water(), resumed.source_high_water());
    assert_donor_result(&input, run_with_fuel(input.as_bytes(), 4_090));
}

#[test]
fn atx_tail_domain_identity_and_terminal_violations_fail_closed() {
    let mut wrong = CountingSource::new(2, b"# alpha\n".to_vec());
    let mut scanner = AtxTailCursorScanner::new(1, wrong.len());
    assert_eq!(
        scanner.poll(&mut wrong, 8),
        Err(AtxTailScanError::WrongSource)
    );
    assert_eq!(
        scanner.poll(&mut wrong, 8),
        Err(AtxTailScanError::PollAfterFailure)
    );

    for bytes in [b"".as_slice(), b" \t\r\n".as_slice()] {
        let mut source = CountingSource::new(3, bytes.to_vec());
        let mut scanner = AtxTailCursorScanner::new(3, source.len());
        assert_eq!(
            scanner.poll(&mut source, 8),
            Err(AtxTailScanError::EmptyAfterTrim)
        );
        assert_eq!(
            scanner.poll(&mut source, 8),
            Err(AtxTailScanError::PollAfterFailure)
        );
    }

    let mut source = CountingSource::new(4, b"#\n".to_vec());
    let mut scanner = AtxTailCursorScanner::new(4, source.len());
    assert_eq!(
        scanner.poll(&mut source, 0),
        Err(AtxTailScanError::ZeroFuel)
    );
    assert!(matches!(
        scanner.poll(&mut source, 8).expect("valid poll").result,
        AtxTailScanResult::Complete(_)
    ));
    assert_eq!(
        scanner.poll(&mut source, 8),
        Err(AtxTailScanError::PollAfterComplete)
    );

    let cuts = run_with_fuel(b"#\n", 1).cuts;
    assert_eq!(
        cuts.with_opener_end(3),
        Err(AtxLineCutsError::OpenerBeyondLine {
            opener_end: 3,
            line_end: 2,
        })
    );
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
