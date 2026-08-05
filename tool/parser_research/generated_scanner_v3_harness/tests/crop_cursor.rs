use std::collections::VecDeque;

use comrak::block_spine_facade as facade;
use flark_v3_runtime_slice::{CropSourceCursor, SOURCE_CURSOR_COPY_CAP_BYTES, SourceStore};
use generated_scanner_gate::{AtxCursorSource, CursorAtxScanner, CursorScanResult};

const REWIND_WINDOW_BYTES: usize = 16;

/// Test adapter for the production source shape: Crop owns the immutable text
/// and its reusable 4 KiB chunk. The grammar cursor preserves the pre-existing
/// conservative 16-byte rewind window while independent instrumentation
/// measures how much backwards source access this generated DFA actually asks
/// for.
struct CropAtxSource {
    source_key: u64,
    source_len: usize,
    cursor: CropSourceCursor,
    rewind_start: usize,
    rewind: VecDeque<u8>,
    accesses: usize,
    first_reads: usize,
    previous_request: Option<usize>,
    maximum_requested_rewind: usize,
}

impl CropAtxSource {
    fn new(text: &str) -> Self {
        let store = SourceStore::new(text, 1);
        let snapshot = store.query_snapshot();
        Self {
            source_key: snapshot.identity().0,
            source_len: snapshot.len_bytes(),
            cursor: snapshot.cursor(),
            rewind_start: 0,
            rewind: VecDeque::with_capacity(REWIND_WINDOW_BYTES),
            accesses: 0,
            first_reads: 0,
            previous_request: None,
            maximum_requested_rewind: 0,
        }
    }

    fn cached_byte(&self, absolute_offset: usize) -> Option<u8> {
        let relative = absolute_offset.checked_sub(self.rewind_start)?;
        self.rewind.get(relative).copied()
    }

    fn push_byte(&mut self, absolute_offset: usize, byte: u8) {
        assert_eq!(absolute_offset, self.cursor.offset() - 1);
        if self.rewind.len() == REWIND_WINDOW_BYTES {
            self.rewind.pop_front();
            self.rewind_start += 1;
        }
        self.rewind.push_back(byte);
    }
}

impl AtxCursorSource for CropAtxSource {
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
        if let Some(byte) = self.cached_byte(absolute_offset) {
            return byte;
        }
        assert_eq!(
            absolute_offset,
            self.cursor.offset(),
            "generated ATX cursor rewound beyond its fixed source window"
        );
        let source = self
            .cursor
            .next_byte()
            .expect("scanner offset is in source");
        assert_eq!(source.root.0, self.source_key);
        assert_eq!(source.offset, absolute_offset);
        self.first_reads += 1;
        self.push_byte(absolute_offset, source.byte);
        source.byte
    }
}

fn run_crop(input: &str, fuel: usize) -> (CursorScanResult, CropAtxSource) {
    let mut source = CropAtxSource::new(input);
    let mut scanner = CursorAtxScanner::new(source.source_key(), source.len());
    loop {
        let receipt = scanner.poll(&mut source, fuel).expect("Crop cursor poll");
        assert!(receipt.source_bytes_inspected <= fuel + receipt.maximum_lookahead_slack);
        match receipt.result.expect("scanner result") {
            CursorScanResult::NeedMore => {}
            terminal => return (terminal, source),
        }
    }
}

#[test]
fn crop_cursor_matches_pinned_comrak_across_tiny_fuel_crlf_and_unicode() {
    let cases = [
        "# heading",
        "######\tβ",
        "######",
        "####### text",
        "#no-space",
        "not a heading",
        "# α\r\n",
        "# 😀\n",
        "##    é",
    ];
    for input in cases {
        let expected = facade::atx_heading_start(input).unwrap();
        for fuel in [1, 2, 7, 4_090] {
            let (result, source) = run_crop(input, fuel);
            let actual = match result {
                CursorScanResult::Matched(bytes) => Some(bytes),
                CursorScanResult::NoMatch => None,
                CursorScanResult::NeedMore => unreachable!(),
            };
            assert_eq!(actual, expected, "input={input:?}, fuel={fuel}");
            assert_eq!(source.first_reads, source.cursor.offset());
            assert_eq!(source.maximum_requested_rewind, 0);
            assert!(source.rewind.len() <= REWIND_WINDOW_BYTES);
            assert!(source.cursor.metrics().maximum_chunk_bytes <= SOURCE_CURSOR_COPY_CAP_BYTES);
        }
    }
}

#[test]
fn ten_mib_generated_atx_scan_runs_directly_over_crop_with_bounded_scratch() {
    const BODY_BYTES: usize = 10 * 1024 * 1024;
    const FUEL: usize = 4090;

    let mut line = String::with_capacity(BODY_BYTES + 8);
    line.push_str("######");
    line.push_str(&" ".repeat(BODY_BYTES));
    line.push('x');

    let mut source = CropAtxSource::new(&line);
    let mut scanner = CursorAtxScanner::new(source.source_key(), source.len());
    let mut polls = 0_usize;
    let matched = loop {
        let receipt = scanner.poll(&mut source, FUEL).expect("Crop ATX poll");
        assert!(receipt.source_bytes_inspected <= FUEL + receipt.maximum_lookahead_slack);
        polls += 1;
        match receipt.result.expect("scanner result") {
            CursorScanResult::NeedMore => {}
            CursorScanResult::Matched(offset) => break offset,
            CursorScanResult::NoMatch => panic!("giant ATX candidate must match"),
        }
    };

    assert_eq!(matched, line.len() - 1);
    assert!(polls > 2_500);
    assert_eq!(source.first_reads, source.cursor.offset());
    assert_eq!(source.maximum_requested_rewind, 0);
    assert!(source.accesses >= source.first_reads);
    assert!(source.rewind.len() <= REWIND_WINDOW_BYTES);
    assert!(source.cursor.metrics().maximum_chunk_bytes <= SOURCE_CURSOR_COPY_CAP_BYTES);
    assert_eq!(scanner.retained_source_bytes(), 0);
}
