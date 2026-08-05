use comrak::block_spine_facade as facade;
use generated_scanner_gate::{
    AtxCursorSource, CursorAtxScanner, CursorScanError, CursorScanResult,
};

struct CountingSource {
    key: u64,
    bytes: Vec<u8>,
    accesses: usize,
}

impl CountingSource {
    fn new(key: u64, bytes: Vec<u8>) -> Self {
        Self {
            key,
            bytes,
            accesses: 0,
        }
    }
}

impl AtxCursorSource for CountingSource {
    fn source_key(&self) -> u64 {
        self.key
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn byte_at(&mut self, absolute_offset: usize) -> u8 {
        self.accesses += 1;
        self.bytes[absolute_offset]
    }
}

fn run(bytes: &[u8], fuel: usize) -> (CursorScanResult, usize, usize) {
    let mut source = CountingSource::new(17, bytes.to_vec());
    let mut scanner = CursorAtxScanner::new(source.source_key(), source.len());
    let mut polls = 0;
    loop {
        let receipt = scanner.poll(&mut source, fuel).expect("cursor scan");
        assert_eq!(receipt.retained_source_bytes, 0);
        assert!(
            receipt.source_bytes_inspected <= fuel + receipt.maximum_lookahead_slack,
            "one poll inspected {} source bytes for fuel {fuel} and slack {}",
            receipt.source_bytes_inspected,
            receipt.maximum_lookahead_slack,
        );
        polls += 1;
        let result = receipt.result.expect("poll result");
        if result != CursorScanResult::NeedMore {
            return (result, polls, source.accesses);
        }
    }
}

#[test]
fn cursor_scanner_matches_pinned_comrak_at_every_small_fuel() {
    let cases = [
        "# heading",
        "######\ttext",
        "######",
        "####### text",
        "#no-space",
        "not a heading",
        "# \r\n",
    ];
    for input in cases {
        let expected = facade::atx_heading_start(input).unwrap();
        for fuel in 1..=input.len().max(1) + 1 {
            let actual = match run(input.as_bytes(), fuel).0 {
                CursorScanResult::Matched(bytes) => Some(bytes),
                CursorScanResult::NoMatch => None,
                CursorScanResult::NeedMore => unreachable!(),
            };
            assert_eq!(actual, expected, "input={input:?}, fuel={fuel}");
        }
    }
}

#[test]
fn cursor_scanner_matches_randomized_comrak_lines() {
    let alphabet = b"# abcXYZ09\t";
    let mut rng = Lcg(0x6375_7273_6f72_6174);
    for case in 0..20_000 {
        let len = rng.usize(192);
        let mut input: String = (0..len)
            .map(|_| char::from(alphabet[rng.usize(alphabet.len())]))
            .collect();
        match case % 4 {
            0 => input.push('\n'),
            1 => input.push_str("\r\n"),
            _ => {}
        }
        let expected = facade::atx_heading_start(&input).unwrap();
        for fuel in [1, 2, 7, 4_090] {
            let actual = match run(input.as_bytes(), fuel).0 {
                CursorScanResult::Matched(bytes) => Some(bytes),
                CursorScanResult::NoMatch => None,
                CursorScanResult::NeedMore => unreachable!(),
            };
            assert_eq!(actual, expected, "case={case}, fuel={fuel}");
        }
    }
}

#[test]
fn ten_mib_cursor_scan_is_bounded_and_retains_no_source() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    const FUEL: usize = 4_090;
    let mut bytes = Vec::with_capacity(TEN_MIB + 3);
    bytes.extend_from_slice(b"# ");
    bytes.extend(std::iter::repeat_n(b' ', TEN_MIB));
    bytes.push(b'x');

    let mut source = CountingSource::new(91, bytes);
    let mut scanner = CursorAtxScanner::new(source.source_key(), source.len());
    assert_eq!(source.accesses, 0, "admission does not touch source");
    assert_eq!(scanner.retained_source_bytes(), 0);

    let mut polls = 0;
    loop {
        let receipt = scanner.poll(&mut source, FUEL).expect("giant ATX poll");
        assert!(receipt.source_bytes_inspected <= 4_096);
        assert_eq!(receipt.retained_source_bytes, 0);
        polls += 1;
        match receipt.result.expect("poll result") {
            CursorScanResult::NeedMore => {}
            CursorScanResult::Matched(bytes) => {
                assert_eq!(bytes, source.len() - 1);
                break;
            }
            CursorScanResult::NoMatch => panic!("giant ATX line must match"),
        }
    }
    assert!(polls > 2_500, "10 MiB scan yielded repeatedly: {polls}");
    assert_eq!(scanner.retained_source_bytes(), 0);
}

#[test]
fn cursor_checkpoint_resumes_against_the_same_source_identity() {
    let bytes = format!("# {}x", " ".repeat(32 * 1024)).into_bytes();
    let mut source = CountingSource::new(7, bytes.clone());
    let mut scanner = CursorAtxScanner::new(source.source_key(), source.len());
    for _ in 0..4 {
        assert_eq!(
            scanner.poll(&mut source, 31).expect("prefix poll").result,
            Some(CursorScanResult::NeedMore)
        );
    }
    let mut resumed = scanner.clone();
    let mut resumed_source = CountingSource::new(7, bytes);

    let left = loop {
        let result = scanner
            .poll(&mut source, 127)
            .expect("original resume")
            .result
            .expect("result");
        if result != CursorScanResult::NeedMore {
            break result;
        }
    };
    let right = loop {
        let result = resumed
            .poll(&mut resumed_source, 1_013)
            .expect("split resume")
            .result
            .expect("result");
        if result != CursorScanResult::NeedMore {
            break result;
        }
    };
    assert_eq!(left, right);
    assert_eq!(scanner.cursor(), resumed.cursor());
}

#[test]
fn cursor_source_identity_and_sentinel_violations_fail_closed() {
    let mut wrong = CountingSource::new(2, b"# heading".to_vec());
    let mut scanner = CursorAtxScanner::new(1, wrong.len());
    assert_eq!(
        scanner.poll(&mut wrong, 32),
        Err(CursorScanError::WrongSource)
    );
    assert_eq!(
        scanner.poll(&mut wrong, 32),
        Err(CursorScanError::PollAfterFailure)
    );

    let mut invalid = CountingSource::new(3, vec![b'#', b' ', 0xff]);
    let mut scanner = CursorAtxScanner::new(3, invalid.len());
    assert_eq!(
        scanner.poll(&mut invalid, 32),
        Err(CursorScanError::SourceContainsSentinel { absolute_offset: 2 })
    );
    assert_eq!(
        scanner.poll(&mut invalid, 32),
        Err(CursorScanError::PollAfterFailure)
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
