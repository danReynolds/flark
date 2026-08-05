use comrak::block_spine_facade as facade;
use flark_v3_runtime_slice::{
    CropSourceCursor, SOURCE_CURSOR_COPY_CAP_BYTES, SourceQuerySnapshot, SourceStore,
};
use generated_scanner_gate::{
    AtxTailCursorScanner, AtxTailCursorSource, AtxTailCuts, AtxTailScanResult,
};

/// Strict zero-cache Crop adapter. A request must be exactly the next physical
/// byte, which makes any hidden backwards read fail immediately.
struct StrictCropAtxTailSource {
    source_key: u64,
    source_len: usize,
    cursor: CropSourceCursor,
    previous_request: Option<usize>,
    maximum_requested_rewind: usize,
    accesses: usize,
    first_reads: usize,
}

impl StrictCropAtxTailSource {
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

    /// Test-only reconstruction of the source-side cursor state. A live
    /// continuation keeps the already-advanced Crop cursor directly.
    fn replay_to(snapshot: &SourceQuerySnapshot, physical_high_water: usize) -> Self {
        let mut source = Self::from_snapshot(snapshot);
        for offset in 0..physical_high_water {
            let _ = source.byte_at(offset);
        }
        source
    }
}

impl AtxTailCursorSource for StrictCropAtxTailSource {
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
            "ATX tail scanner requested a non-forward source byte"
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
    AtxTailCuts,
    AtxTailCursorScanner,
    StrictCropAtxTailSource,
    usize,
) {
    let mut source = StrictCropAtxTailSource::new(input);
    let mut scanner = AtxTailCursorScanner::new(source.source_key(), source.len());
    let mut polls = 0;
    loop {
        let receipt = scanner.poll(&mut source, fuel).expect("Crop ATX tail poll");
        assert!(receipt.source_byte_requests <= fuel);
        assert_eq!(receipt.maximum_source_request_rewind_bytes, 0);
        assert_eq!(receipt.retained_source_bytes, 0);
        polls += 1;
        match receipt.result {
            AtxTailScanResult::NeedMore => {}
            AtxTailScanResult::Complete(cuts) => return (cuts, scanner, source, polls),
        }
    }
}

fn assert_strict_source_receipt(scanner: &AtxTailCursorScanner, source: &StrictCropAtxTailSource) {
    assert_eq!(source.accesses, source.first_reads);
    assert_eq!(source.first_reads, source.cursor.offset());
    assert_eq!(source.maximum_requested_rewind, 0);
    assert_eq!(scanner.cursor(), source.cursor.offset());
    assert_eq!(scanner.source_high_water(), source.cursor.offset());
    assert!(source.cursor.metrics().maximum_chunk_bytes <= SOURCE_CURSOR_COPY_CAP_BYTES);
    assert_eq!(scanner.retained_source_bytes(), 0);
}

#[test]
fn strict_crop_atx_tail_matches_donor_for_line_endings_unicode_and_tiny_fuel() {
    let cases = [
        "# alpha",
        "# alpha\n",
        "# alpha\r",
        "# alpha\r\n",
        "# alpha#   \n",
        "# alpha ###   \r\n",
        "# β\t###\r",
        "# 😀#   ",
        "# alpha\u{b}",
        "# alpha\u{c}",
        "# alpha\u{a0}",
    ];
    for input in cases {
        let (expected, closed) = facade::chop_trailing_hashes(input).expect("donor facade");
        for fuel in [1, 2, 7, 4_090] {
            let (cuts, scanner, source, _) = run_crop(input, fuel);
            assert_eq!(cuts.chopped_end(), expected.len(), "input={input:?}");
            assert_eq!(cuts.closed(), closed, "input={input:?}");
            assert_strict_source_receipt(&scanner, &source);
        }
    }
}

#[test]
fn strict_crop_atx_tail_clone_resumes_from_physical_high_water() {
    let input = format!("# {} ###   \r\n", "β".repeat(16 * 1024));
    let store = SourceStore::new(&input, 1);
    let snapshot = store.query_snapshot();
    let mut source = StrictCropAtxTailSource::from_snapshot(&snapshot);
    let mut scanner = AtxTailCursorScanner::new(source.source_key(), source.len());
    for _ in 0..8 {
        assert_eq!(
            scanner.poll(&mut source, 17).expect("prefix poll").result,
            AtxTailScanResult::NeedMore
        );
    }
    let physical_high_water = scanner.source_high_water();
    assert_eq!(physical_high_water, source.cursor.offset());

    let mut resumed = scanner.clone();
    let mut resumed_source = StrictCropAtxTailSource::replay_to(&snapshot, physical_high_water);
    let left = loop {
        match scanner
            .poll(&mut source, 31)
            .expect("original Crop resume")
            .result
        {
            AtxTailScanResult::NeedMore => {}
            AtxTailScanResult::Complete(cuts) => break cuts,
        }
    };
    let right = loop {
        match resumed
            .poll(&mut resumed_source, 1_013)
            .expect("replayed Crop resume")
            .result
        {
            AtxTailScanResult::NeedMore => {}
            AtxTailScanResult::Complete(cuts) => break cuts,
        }
    };
    assert_eq!(left, right);
    assert_eq!(scanner.cursor(), resumed.cursor());
    assert_eq!(scanner.source_high_water(), resumed.source_high_water());
    assert_strict_source_receipt(&scanner, &source);
    assert_strict_source_receipt(&resumed, &resumed_source);
}

#[test]
fn ten_mib_atx_tail_cases_are_forward_bounded_and_source_free() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    const FUEL: usize = 4_090;

    for case in 0..3 {
        let (line, expected_chopped_end, expected_closed, expected_content_end) = match case {
            0 => {
                let mut line = String::with_capacity(TEN_MIB + 16);
                line.push_str("# ");
                line.extend(std::iter::repeat_n('a', TEN_MIB));
                let chopped_end = line.len();
                line.push_str(" ###   \r\n");
                let content_end = line.len() - 2;
                (line, chopped_end, true, content_end)
            }
            1 => {
                let mut line = String::with_capacity(TEN_MIB + 16);
                line.push_str("# ");
                line.extend(std::iter::repeat_n('a', TEN_MIB));
                line.push('#');
                let chopped_end = line.len();
                line.push_str("   \n");
                let content_end = line.len() - 1;
                (line, chopped_end, false, content_end)
            }
            _ => {
                let mut line = String::with_capacity(TEN_MIB + 4);
                line.push_str("# α");
                let chopped_end = line.len();
                line.extend(std::iter::repeat_n(' ', TEN_MIB));
                let content_end = line.len();
                (line, chopped_end, false, content_end)
            }
        };

        let (cuts, scanner, source, polls) = run_crop(&line, FUEL);
        assert_eq!(cuts.chopped_end(), expected_chopped_end);
        assert_eq!(cuts.closed(), expected_closed);
        assert_eq!(cuts.content_end(), expected_content_end);
        assert_eq!(cuts.line_end(), line.len());
        assert!(polls > 2_500, "10 MiB ATX tail must yield");
        assert_eq!(scanner.source_high_water(), line.len());
        assert_strict_source_receipt(&scanner, &source);

        let whole = cuts.with_opener_end(2).expect("known ATX opener cut");
        assert_eq!(whole.marker_end(), 2);
        assert_eq!(whole.visible_end(), expected_chopped_end.max(2));
        assert_eq!(whole.content_end(), expected_content_end);
        assert_eq!(whole.closed(), expected_closed);
        if case == 2 {
            assert!(scanner.source_high_water() - cuts.chopped_end() >= TEN_MIB);
        }
    }
}
