use std::cmp::Ordering;

use flark_engine::{SourceStore, SOURCE_CURSOR_WINDOW_BYTES};
use flark_parser::{
    M11LineEnding, M11SourceLineSource, SnapshotLinePoll, SnapshotLineScanner, SnapshotLineSource,
    SourceAdapterError,
};

struct ScannedLine {
    facts: flark_parser::M11PhysicalLineFacts,
    bytes: Vec<u8>,
    refill_count: usize,
    max_refill_bytes: usize,
}

fn first_line_source(text: &str, fuel: usize) -> SnapshotLineSource {
    let store = SourceStore::new(text).expect("source");
    let scanner = SnapshotLineScanner::new(store.snapshot()).expect("line scanner");
    next_line_source(scanner, fuel).expect("source must have at least its EOF line")
}

fn next_line_source(mut scanner: SnapshotLineScanner, fuel: usize) -> Option<SnapshotLineSource> {
    loop {
        match scanner.poll(fuel).expect("line poll") {
            SnapshotLinePoll::Pending(next) => scanner = next,
            SnapshotLinePoll::Line(line) => {
                return Some(line.into_source().expect("line source"));
            }
            SnapshotLinePoll::Complete => return None,
        }
    }
}

fn scan_lines(text: &str, fuel: usize) -> Vec<ScannedLine> {
    let store = SourceStore::new(text).expect("source");
    let mut scanner = SnapshotLineScanner::new(store.snapshot()).expect("line scanner");
    let mut lines = Vec::new();
    loop {
        match scanner.poll(fuel).expect("line poll") {
            SnapshotLinePoll::Pending(next) => scanner = next,
            SnapshotLinePoll::Line(line) => {
                let facts = line.facts();
                let (bytes, source) = read_all(line.into_source().expect("line source"));
                let refill_count = source.refill_count();
                let max_refill_bytes = source.max_refill_bytes();
                scanner = source.finish().expect("complete line returns read baton");
                lines.push(ScannedLine {
                    facts,
                    bytes,
                    refill_count,
                    max_refill_bytes,
                });
            }
            SnapshotLinePoll::Complete => return lines,
        }
    }
}

fn read_all(mut source: SnapshotLineSource) -> (Vec<u8>, SnapshotLineSource) {
    let mut bytes = Vec::with_capacity(source.len());
    while source.position() < source.len() {
        let grant = source
            .replenish_access_budget(SOURCE_CURSOR_WINDOW_BYTES)
            .expect("fresh bounded grant");
        assert!(grant <= SOURCE_CURSOR_WINDOW_BYTES);
        for _ in 0..grant {
            let offset = source.position();
            bytes.push(source.read_byte(offset).expect("sequential source byte"));
        }
    }
    (bytes, source)
}

#[test]
fn clean_bytes_and_source_adapter_have_exact_line_parity() {
    let cases = [
        "",
        "plain paragraph",
        "first\nsecond\rthird\r\nfourth",
        "[x]: /target \"title\"\r\nvisible paragraph\r\n",
        "[not a definition\r\nliteral text",
    ];

    for text in cases {
        let lines = scan_lines(text, 3);
        assert!(!lines.is_empty());
        for line in lines {
            let facts = line.facts;
            let identity = facts.identity();
            let start = usize::try_from(identity.start_byte()).expect("u32 source start");
            let end = usize::try_from(identity.end_byte()).expect("u32 source end");
            let expected = &text.as_bytes()[start..end];
            assert_eq!(line.bytes, expected);
            assert!(line.max_refill_bytes <= SOURCE_CURSOR_WINDOW_BYTES);
        }
    }
}

#[test]
fn line_authority_preserves_empty_lf_cr_and_crlf() {
    let cases = [
        ("", vec![(0, 0, M11LineEnding::Eof)]),
        ("a\n", vec![(1, 2, M11LineEnding::Lf)]),
        ("a\r", vec![(1, 2, M11LineEnding::Cr)]),
        ("a\r\n", vec![(1, 3, M11LineEnding::CrLf)]),
    ];

    for (text, expected) in cases {
        let actual = scan_lines(text, 1)
            .iter()
            .map(|line| {
                let facts = line.facts;
                (
                    facts.content_bytes(),
                    facts.physical_bytes(),
                    facts.ending(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{text:?}");
    }
}

#[test]
fn unicode_scalar_crossing_window_and_crlf_keep_exact_metrics() {
    let mut text = "a".repeat(SOURCE_CURSOR_WINDOW_BYTES - 1);
    text.push('🦀');
    text.push_str("\r\n");

    let mut lines = scan_lines(&text, 17);
    assert_eq!(lines.len(), 1);
    let line = lines.pop().expect("one line");
    let facts = line.facts;
    assert_eq!(facts.content_bytes(), 4_099);
    assert_eq!(facts.physical_bytes(), 4_101);
    assert_eq!(facts.content_utf16(), 4_097);
    assert_eq!(facts.physical_utf16(), 4_099);
    assert_eq!(facts.ending(), M11LineEnding::CrLf);

    assert_eq!(line.bytes, text.as_bytes());
    assert_eq!(line.max_refill_bytes, SOURCE_CURSOR_WINDOW_BYTES - 1);
    assert!(line.refill_count >= 2);
}

#[test]
fn reference_shaped_lines_are_transported_without_classification() {
    let text = "[x]: /target \"title\"\r\nvisible\n[broken]: <unterminated\r\n";
    let lines = scan_lines(text, 5);
    assert_eq!(lines.len(), 3);

    let mut joined = Vec::new();
    for line in lines {
        joined.extend(line.bytes);
    }
    assert_eq!(joined, text.as_bytes());
}

#[test]
fn ten_mib_line_never_exceeds_the_four_kib_source_window() {
    const CONTENT_BYTES: usize = 10 * 1024 * 1024;
    let mut text = "p".repeat(CONTENT_BYTES);
    text.push_str("\r\n");

    let mut source = first_line_source(&text, SOURCE_CURSOR_WINDOW_BYTES);
    let facts = source.facts();
    assert_eq!(
        usize::try_from(facts.content_bytes()).expect("u32 content"),
        CONTENT_BYTES
    );
    assert_eq!(
        usize::try_from(facts.physical_bytes()).expect("u32 physical"),
        CONTENT_BYTES + 2
    );
    assert_eq!(
        usize::try_from(facts.physical_utf16()).expect("u32 UTF-16"),
        CONTENT_BYTES + 2
    );
    assert_eq!(facts.ending(), M11LineEnding::CrLf);

    while source.position() < source.len() {
        let grant = source
            .replenish_access_budget(usize::MAX)
            .expect("one bounded access grant");
        assert!(grant <= SOURCE_CURSOR_WINDOW_BYTES);
        for _ in 0..grant {
            let offset = source.position();
            let byte = source.read_byte(offset).expect("sequential source byte");
            let expected = match offset.cmp(&CONTENT_BYTES) {
                Ordering::Less => b'p',
                Ordering::Equal => b'\r',
                Ordering::Greater => b'\n',
            };
            assert_eq!(byte, expected);
        }
    }
    assert_eq!(source.position(), CONTENT_BYTES + 2);
    assert!(source.max_refill_bytes() <= SOURCE_CURSOR_WINDOW_BYTES);
    assert!(source.refill_count() >= CONTENT_BYTES / SOURCE_CURSOR_WINDOW_BYTES);
}

#[test]
fn source_borrow_can_be_cancelled_at_every_segment_boundary() {
    let mut text = "z".repeat(SOURCE_CURSOR_WINDOW_BYTES * 5 + 37);
    text.push('\n');
    let len = text.len();
    let mut boundaries = (0..=len)
        .step_by(SOURCE_CURSOR_WINDOW_BYTES)
        .collect::<Vec<_>>();
    if boundaries.last().copied() != Some(len) {
        boundaries.push(len);
    }

    for boundary in boundaries {
        let mut source = first_line_source(&text, 101);
        let identity = source.facts().identity();
        while source.position() < boundary {
            let request = (boundary - source.position()).min(SOURCE_CURSOR_WINDOW_BYTES);
            let grant = source
                .replenish_access_budget(request)
                .expect("bounded grant");
            for _ in 0..grant {
                let offset = source.position();
                let _ = source.read_byte(offset).expect("source byte");
            }
        }
        let (cancellation, scanner) = source.cancel();
        assert_eq!(cancellation.identity, identity);
        assert_eq!(cancellation.bytes_read, boundary);
        assert_eq!(cancellation.unused_access_budget, 0);
        drop(scanner);

        let replay = first_line_source(&text, 101);
        let (replayed, _) = read_all(replay);
        assert_eq!(replayed, text.as_bytes());
    }
}

#[test]
fn budget_and_sequence_violations_fail_typed() {
    let mut source = first_line_source("abc", 8);

    assert_eq!(
        source.read_byte(0),
        Err(SourceAdapterError::AccessBudgetExhausted)
    );
    assert_eq!(source.replenish_access_budget(2), Ok(2));
    assert_eq!(
        source.replenish_access_budget(2),
        Err(SourceAdapterError::OutstandingAccessBudget { remaining: 2 })
    );
    assert_eq!(
        source.read_byte(1),
        Err(SourceAdapterError::NonSequentialRead {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(source.read_byte(0), Ok(b'a'));
    assert_eq!(source.read_byte(1), Ok(b'b'));
    assert_eq!(
        source.read_byte(2),
        Err(SourceAdapterError::AccessBudgetExhausted)
    );
}

#[test]
fn cancelling_a_line_returns_the_only_baton_for_later_discovery() {
    let mut first = first_line_source("first\nsecond", 1);
    first.replenish_access_budget(2).expect("read grant");
    assert_eq!(first.read_byte(0), Ok(b'f'));
    assert_eq!(first.read_byte(1), Ok(b'i'));
    let (receipt, scanner) = first.cancel();
    assert_eq!(receipt.bytes_read, 2);

    let second = next_line_source(scanner, 1).expect("second physical line");
    assert_eq!(second.facts().identity().ordinal(), 1);
    let (bytes, second) = read_all(second);
    assert_eq!(bytes, b"second");
    let scanner = second.finish().expect("complete second line");
    assert!(next_line_source(scanner, 1).is_none());
}
