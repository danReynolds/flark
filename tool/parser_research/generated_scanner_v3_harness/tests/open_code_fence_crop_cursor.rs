use comrak::block_spine_facade as facade;
use flark_v3_runtime_slice::{
    CropSourceCursor, SOURCE_CURSOR_COPY_CAP_BYTES, SourceQuerySnapshot, SourceStore,
};
use generated_scanner_gate::{
    OpenCodeFenceCursorScanner, OpenCodeFenceCursorSource, OpenCodeFenceScanResult,
};

/// Strict forward-only Crop adapter for the fence DFA. It retains no source
/// bytes and rejects any request other than the exact next Crop byte.
/// Instrumentation measures requested rewind independently from whether the
/// assertion passes.
struct StrictCropFenceSource {
    source_key: u64,
    source_len: usize,
    cursor: CropSourceCursor,
    previous_request: Option<usize>,
    maximum_requested_rewind: usize,
    accesses: usize,
    first_reads: usize,
}

impl StrictCropFenceSource {
    fn new(text: &str) -> Self {
        let store = SourceStore::new(text, 1);
        let snapshot = store.query_snapshot();
        Self::from_snapshot(&snapshot)
    }

    fn from_snapshot(snapshot: &SourceQuerySnapshot) -> Self {
        Self {
            source_key: snapshot.identity().0,
            source_len: snapshot.len_bytes(),
            cursor: snapshot.cursor(),
            previous_request: None,
            maximum_requested_rewind: 0,
            accesses: 0,
            first_reads: 0,
        }
    }

    /// Reconstructs the exact source-side continuation state from the same
    /// immutable snapshot. Replaying here is only a test mechanism: it proves
    /// that physical high-water alone is enough on the source side; a live
    /// continuation keeps the already-advanced Crop cursor directly.
    fn replay_to(snapshot: &SourceQuerySnapshot, physical_high_water: usize) -> Self {
        let mut source = Self::from_snapshot(snapshot);
        for offset in 0..physical_high_water {
            let _ = source.byte_at(offset);
        }
        source
    }
}

impl OpenCodeFenceCursorSource for StrictCropFenceSource {
    fn source_key(&self) -> u64 {
        self.source_key
    }

    fn len(&self) -> usize {
        self.source_len
    }

    fn byte_at(&mut self, absolute_offset: usize) -> u8 {
        self.accesses += 1;
        if let Some(previous) = self.previous_request {
            self.maximum_requested_rewind = self
                .maximum_requested_rewind
                .max(previous.saturating_sub(absolute_offset));
        }
        self.previous_request = Some(absolute_offset);

        assert_eq!(
            absolute_offset,
            self.cursor.offset(),
            "generated fence scanner requested a non-forward source byte"
        );
        let source = self
            .cursor
            .next_byte()
            .expect("scanner offset is inside the Crop source");
        assert_eq!(source.root.0, self.source_key);
        assert_eq!(source.offset, absolute_offset);
        self.first_reads += 1;
        source.byte
    }
}

fn run_crop(
    input: &str,
    fuel: usize,
) -> (
    OpenCodeFenceScanResult,
    OpenCodeFenceCursorScanner,
    StrictCropFenceSource,
    usize,
) {
    let mut source = StrictCropFenceSource::new(input);
    let mut scanner = OpenCodeFenceCursorScanner::new(source.source_key(), source.len());
    let mut polls = 0;
    loop {
        let receipt = scanner.poll(&mut source, fuel).expect("Crop fence poll");
        assert!(receipt.source_byte_requests <= fuel + receipt.maximum_lookahead_slack);
        assert_eq!(receipt.maximum_source_request_rewind_bytes, 0);
        assert_eq!(receipt.retained_source_bytes, 0);
        polls += 1;
        match receipt.result.expect("poll result") {
            OpenCodeFenceScanResult::NeedMore => {}
            result => return (result, scanner, source, polls),
        }
    }
}

fn expected_result(expected: Option<usize>) -> OpenCodeFenceScanResult {
    expected.map_or(OpenCodeFenceScanResult::NoMatch, |bytes| {
        OpenCodeFenceScanResult::Matched(bytes)
    })
}

fn assert_strict_source_receipt(
    scanner: &OpenCodeFenceCursorScanner,
    source: &StrictCropFenceSource,
) {
    assert_eq!(source.first_reads, source.cursor.offset());
    assert_eq!(source.maximum_requested_rewind, 0);
    assert_eq!(scanner.maximum_source_request_rewind_bytes(), 0);
    assert_eq!(source.accesses, source.first_reads);
    assert!(source.cursor.metrics().maximum_chunk_bytes <= SOURCE_CURSOR_COPY_CAP_BYTES);
    assert_eq!(scanner.retained_source_bytes(), 0);
}

#[test]
fn strict_crop_fence_cursor_matches_comrak_at_tiny_fuel_crlf_and_unicode() {
    let cases = [
        "```",
        "```` rust",
        "```rust\n",
        "```ru`st\n",
        "~~~~ rust`allowed\r\n",
        "~~ nope",
        "not a fence",
        "``` 😀\n",
        "~~~ β\r",
    ];
    for input in cases {
        let expected = expected_result(facade::open_code_fence(input).unwrap());
        for fuel in [1, 2, 7, 4_090] {
            let (actual, scanner, source, _) = run_crop(input, fuel);
            assert_eq!(actual, expected, "input={input:?}, fuel={fuel}");
            assert_strict_source_receipt(&scanner, &source);
        }
    }
}

#[test]
fn strict_crop_fence_cursor_clone_resumes_from_physical_high_water() {
    let input = format!("```{}\n", "a".repeat(32 * 1024));
    let store = SourceStore::new(&input, 1);
    let snapshot = store.query_snapshot();
    let mut source = StrictCropFenceSource::from_snapshot(&snapshot);
    let mut scanner = OpenCodeFenceCursorScanner::new(source.source_key(), source.len());

    for _ in 0..8 {
        assert_eq!(
            scanner.poll(&mut source, 17).expect("prefix poll").result,
            Some(OpenCodeFenceScanResult::NeedMore)
        );
    }
    let physical_high_water = scanner.source_high_water();
    assert_eq!(source.cursor.offset(), physical_high_water);

    let mut resumed = scanner.clone();
    let mut resumed_source = StrictCropFenceSource::replay_to(&snapshot, physical_high_water);
    let left = loop {
        let result = scanner
            .poll(&mut source, 31)
            .expect("original Crop resume")
            .result
            .expect("result");
        if result != OpenCodeFenceScanResult::NeedMore {
            break result;
        }
    };
    let right = loop {
        let result = resumed
            .poll(&mut resumed_source, 1_013)
            .expect("replayed Crop resume")
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
    assert_strict_source_receipt(&scanner, &source);
    assert_strict_source_receipt(&resumed, &resumed_source);
}

#[test]
fn ten_mib_fence_trailing_context_needs_zero_cached_bytes_and_zero_source_rewind() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    const FUEL: usize = 4_090;

    for (prefix, suffix, expected) in [
        ("```", "\n", Some(3)),
        ("~~~", "\r\n", Some(3)),
        ("```", "`\n", None),
    ] {
        let mut line = String::with_capacity(prefix.len() + TEN_MIB + suffix.len());
        line.push_str(prefix);
        line.extend(std::iter::repeat_n('a', TEN_MIB));
        line.push_str(suffix);

        let (actual, scanner, source, polls) = run_crop(&line, FUEL);
        assert_eq!(actual, expected_result(expected));
        assert!(polls > 2_500, "10 MiB trailing context must yield");
        assert!(scanner.source_high_water() >= TEN_MIB);
        assert!(scanner.maximum_logical_rewind_bytes() >= TEN_MIB);
        assert_eq!(scanner.cursor(), expected.unwrap_or(1));
        assert_strict_source_receipt(&scanner, &source);
    }
}
