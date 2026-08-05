use comrak::block_spine_facade as facade;
use generated_scanner_gate::{
    OpenCodeFenceCursorScanner, OpenCodeFenceCursorSource, OpenCodeFenceScanError,
    OpenCodeFenceScanResult,
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

impl OpenCodeFenceCursorSource for CountingSource {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RunReceipt {
    result: OpenCodeFenceScanResult,
    polls: usize,
    source_byte_requests: usize,
    cursor: usize,
    source_high_water: usize,
    maximum_logical_rewind_bytes: usize,
    maximum_source_request_rewind_bytes: usize,
}

fn run_with_fuel(bytes: &[u8], fuel: usize) -> RunReceipt {
    let mut source = CountingSource::new(17, bytes.to_vec());
    let mut scanner = OpenCodeFenceCursorScanner::new(source.source_key(), source.len());
    let mut polls = 0;
    let mut source_byte_requests = 0;
    loop {
        let receipt = scanner.poll(&mut source, fuel).expect("fence cursor scan");
        assert_eq!(receipt.retained_source_bytes, 0);
        assert!(
            receipt.source_byte_requests <= fuel + receipt.maximum_lookahead_slack,
            "one poll requested {} source bytes for fuel {fuel} and slack {}",
            receipt.source_byte_requests,
            receipt.maximum_lookahead_slack,
        );
        source_byte_requests += receipt.source_byte_requests;
        polls += 1;
        let result = receipt.result.expect("poll result");
        if result != OpenCodeFenceScanResult::NeedMore {
            assert_eq!(source.accesses, source_byte_requests);
            return RunReceipt {
                result,
                polls,
                source_byte_requests,
                cursor: scanner.cursor(),
                source_high_water: scanner.source_high_water(),
                maximum_logical_rewind_bytes: scanner.maximum_logical_rewind_bytes(),
                maximum_source_request_rewind_bytes: scanner.maximum_source_request_rewind_bytes(),
            };
        }
    }
}

fn run_with_random_fuel(bytes: &[u8], rng: &mut Lcg) -> RunReceipt {
    let mut source = CountingSource::new(19, bytes.to_vec());
    let mut scanner = OpenCodeFenceCursorScanner::new(source.source_key(), source.len());
    let mut polls = 0;
    let mut source_byte_requests = 0;
    loop {
        let fuel = rng.usize(31) + 1;
        let receipt = scanner
            .poll(&mut source, fuel)
            .expect("random-fuel fence cursor scan");
        assert!(receipt.source_byte_requests <= fuel + receipt.maximum_lookahead_slack);
        source_byte_requests += receipt.source_byte_requests;
        polls += 1;
        let result = receipt.result.expect("poll result");
        if result != OpenCodeFenceScanResult::NeedMore {
            assert_eq!(source.accesses, source_byte_requests);
            return RunReceipt {
                result,
                polls,
                source_byte_requests,
                cursor: scanner.cursor(),
                source_high_water: scanner.source_high_water(),
                maximum_logical_rewind_bytes: scanner.maximum_logical_rewind_bytes(),
                maximum_source_request_rewind_bytes: scanner.maximum_source_request_rewind_bytes(),
            };
        }
    }
}

fn expected_result(expected: Option<usize>) -> OpenCodeFenceScanResult {
    expected.map_or(OpenCodeFenceScanResult::NoMatch, |bytes| {
        OpenCodeFenceScanResult::Matched(bytes)
    })
}

#[test]
fn fence_cursor_matches_pinned_comrak_at_every_tiny_fuel() {
    let cases = [
        "```",
        "```` rust",
        "```rust\n",
        "```ru`st\n",
        "~~~",
        "~~~~ rust`allowed\r\n",
        "~~ nope",
        "`not a fence",
        "not a fence",
        "``` 😀\n",
        "~~~ β\r",
    ];
    for input in cases {
        let expected = expected_result(facade::open_code_fence(input).unwrap());
        for fuel in 1..=input.len().max(1) + 1 {
            let actual = run_with_fuel(input.as_bytes(), fuel);
            assert_eq!(actual.result, expected, "input={input:?}, fuel={fuel}");
            assert_eq!(
                actual.maximum_source_request_rewind_bytes, 0,
                "the source access sequence must stay monotonic for {input:?}"
            );
        }
    }
}

#[test]
fn fence_cursor_matches_randomized_comrak_lines_and_fuel() {
    let alphabet = b"`~ abcXYZ09\t";
    let mut input_rng = Lcg(0x6665_6e63_655f_696e);
    let mut fuel_rng = Lcg(0x6665_6e63_655f_6675);
    for case in 0..20_000 {
        let len = input_rng.usize(192);
        let mut input: String = (0..len)
            .map(|_| char::from(alphabet[input_rng.usize(alphabet.len())]))
            .collect();
        match case % 4 {
            0 => input.push('\n'),
            1 => input.push_str("\r\n"),
            2 => input.push('\r'),
            _ => {}
        }
        let expected = expected_result(facade::open_code_fence(&input).unwrap());
        for fuel in [1, 2, 7, 4_090] {
            let actual = run_with_fuel(input.as_bytes(), fuel);
            assert_eq!(actual.result, expected, "case={case}, fuel={fuel}");
            assert_eq!(actual.maximum_source_request_rewind_bytes, 0);
        }
        let random = run_with_random_fuel(input.as_bytes(), &mut fuel_rng);
        assert_eq!(random.result, expected, "case={case}, random fuel");
        assert_eq!(random.maximum_source_request_rewind_bytes, 0);
    }
}

#[test]
fn fence_cursor_clone_resumes_trailing_context_under_a_new_schedule() {
    let input = format!("```{}\n", "a".repeat(32 * 1024));
    let mut source = CountingSource::new(29, input.as_bytes().to_vec());
    let mut scanner = OpenCodeFenceCursorScanner::new(source.source_key(), source.len());
    for _ in 0..8 {
        assert_eq!(
            scanner.poll(&mut source, 17).expect("prefix poll").result,
            Some(OpenCodeFenceScanResult::NeedMore)
        );
    }
    let mut resumed = scanner.clone();
    let mut resumed_source = CountingSource::new(29, input.into_bytes());

    let left = loop {
        let result = scanner
            .poll(&mut source, 31)
            .expect("original resume")
            .result
            .expect("result");
        if result != OpenCodeFenceScanResult::NeedMore {
            break result;
        }
    };
    let right = loop {
        let result = resumed
            .poll(&mut resumed_source, 1_013)
            .expect("clone resume")
            .result
            .expect("result");
        if result != OpenCodeFenceScanResult::NeedMore {
            break result;
        }
    };
    assert_eq!(left, right);
    assert_eq!(scanner.cursor(), resumed.cursor());
    assert_eq!(scanner.source_high_water(), resumed.source_high_water());
    assert_eq!(
        scanner.maximum_logical_rewind_bytes(),
        resumed.maximum_logical_rewind_bytes()
    );
    assert_eq!(scanner.maximum_source_request_rewind_bytes(), 0);
    assert_eq!(resumed.maximum_source_request_rewind_bytes(), 0);
}

#[test]
fn ten_mib_trailing_context_exposes_logical_but_not_source_rewind() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    const FUEL: usize = 4_090;

    for (prefix, suffix, expected) in [
        (b"```".as_slice(), b"\n".as_slice(), Some(3)),
        (b"~~~".as_slice(), b"\r\n".as_slice(), Some(3)),
        (b"```".as_slice(), b"`\n".as_slice(), None),
    ] {
        let mut bytes = Vec::with_capacity(prefix.len() + TEN_MIB + suffix.len());
        bytes.extend_from_slice(prefix);
        bytes.extend(std::iter::repeat_n(b'a', TEN_MIB));
        bytes.extend_from_slice(suffix);
        let run = run_with_fuel(&bytes, FUEL);
        assert_eq!(run.result, expected_result(expected));
        assert!(run.polls > 2_500, "10 MiB trailing context must yield");
        assert!(run.source_high_water >= TEN_MIB);
        assert!(run.maximum_logical_rewind_bytes >= TEN_MIB);
        assert_eq!(run.maximum_source_request_rewind_bytes, 0);
        assert_eq!(run.cursor, expected.unwrap_or(1));
    }
}

#[test]
fn fence_cursor_identity_sentinel_and_terminal_violations_fail_closed() {
    let mut wrong = CountingSource::new(2, b"```\n".to_vec());
    let mut scanner = OpenCodeFenceCursorScanner::new(1, wrong.len());
    assert_eq!(
        scanner.poll(&mut wrong, 32),
        Err(OpenCodeFenceScanError::WrongSource)
    );
    assert_eq!(
        scanner.poll(&mut wrong, 32),
        Err(OpenCodeFenceScanError::PollAfterFailure)
    );

    let mut invalid = CountingSource::new(3, vec![b'`', b'`', b'`', b' ', 0xff]);
    let mut scanner = OpenCodeFenceCursorScanner::new(3, invalid.len());
    assert_eq!(
        scanner.poll(&mut invalid, 32),
        Err(OpenCodeFenceScanError::SourceContainsSentinel { absolute_offset: 4 })
    );
    assert_eq!(
        scanner.poll(&mut invalid, 32),
        Err(OpenCodeFenceScanError::PollAfterFailure)
    );

    let mut source = CountingSource::new(4, b"~~~\n".to_vec());
    let mut scanner = OpenCodeFenceCursorScanner::new(4, source.len());
    assert_eq!(
        scanner.poll(&mut source, 0),
        Err(OpenCodeFenceScanError::ZeroFuel)
    );
    assert_eq!(
        scanner.poll(&mut source, 32).expect("valid poll").result,
        Some(OpenCodeFenceScanResult::Matched(3))
    );
    assert_eq!(
        scanner.poll(&mut source, 32),
        Err(OpenCodeFenceScanError::PollAfterComplete)
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
