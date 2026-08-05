use comrak::block_spine_facade::oracle_block_projection;
use flark_oversized_block_line_gate::{
    CancellationToken, DEFAULT_POLL_BYTES, FenceJob, FenceMode, HtmlEndJob, HtmlType7Job, Poll,
    ReferencePrefixJob, TableRowJob,
};

const MIB: usize = 1024 * 1024;

fn complete<T>(mut poll: impl FnMut() -> Poll<T>) -> T {
    loop {
        match poll() {
            Poll::Pending { .. } => {}
            Poll::Ready { value, .. } => return value,
            Poll::Cancelled { .. } => panic!("unexpected cancellation"),
        }
    }
}

#[test]
fn giant_fence_closer_closes_before_the_following_paragraph() {
    let closer = format!("```{}\n", " ".repeat(MIB));
    let token = CancellationToken::default();
    let mut job = FenceJob::new(closer.as_bytes(), FenceMode::Close);
    assert_eq!(
        complete(|| job.poll(closer.as_bytes(), DEFAULT_POLL_BYTES, &token)),
        Some(3)
    );
    assert!(job.receipt().maximum_bytes_per_poll <= DEFAULT_POLL_BYTES);

    let document = format!("```\ninside\n{closer}after\n");
    let blocks = oracle_block_projection(&document, false);
    assert!(
        blocks
            .iter()
            .any(|block| block.kind.starts_with("code:true:"))
    );
    assert!(
        blocks
            .iter()
            .any(|block| block.kind == "paragraph" && block.logical == "after\n")
    );
}

#[test]
fn giant_html_terminator_closes_before_the_following_paragraph() {
    let terminator = format!("{}-->\n", "x".repeat(MIB));
    let token = CancellationToken::default();
    let mut job = HtmlEndJob::new(terminator.as_bytes(), 2);
    assert!(complete(|| job.poll(
        terminator.as_bytes(),
        DEFAULT_POLL_BYTES,
        &token
    )));

    let document = format!("<!--\n{terminator}after\n");
    let blocks = oracle_block_projection(&document, false);
    assert!(blocks.iter().any(|block| block.kind.starts_with("html:2:")));
    assert!(
        blocks
            .iter()
            .any(|block| block.kind == "paragraph" && block.logical == "after\n")
    );
}

#[test]
fn giant_reference_is_removed_without_consuming_the_following_paragraph() {
    let definition = format!("[label]: /{}\n", "u".repeat(MIB));
    let token = CancellationToken::default();
    let mut job = ReferencePrefixJob::new();
    let shape = complete(|| job.poll(definition.as_bytes(), DEFAULT_POLL_BYTES, &token))
        .expect("valid definition");
    assert_eq!(shape.source.end, definition.len());

    let document = format!("{definition}after\n");
    let blocks = oracle_block_projection(&document, false);
    let paragraphs = blocks
        .iter()
        .filter(|block| block.kind == "paragraph")
        .collect::<Vec<_>>();
    assert_eq!(paragraphs.len(), 1);
    assert_eq!(paragraphs[0].logical, "after\n");
}

#[test]
fn giant_table_delimiter_activates_table_then_releases_tail() {
    let delimiter = format!("| :{}: | --- |\n", "-".repeat(MIB));
    let token = CancellationToken::default();
    let mut job = TableRowJob::new(delimiter.as_bytes());
    let row =
        complete(|| job.poll(delimiter.as_bytes(), DEFAULT_POLL_BYTES, &token)).expect("valid row");
    assert_eq!(row.delimiter_alignments, Some(vec![2, 0]));

    let document = format!("| left | right |\n{delimiter}| one | two |\n\nafter\n");
    let blocks = oracle_block_projection(&document, true);
    assert!(blocks.iter().any(|block| block.kind.starts_with("table:")));
    assert!(
        blocks
            .iter()
            .any(|block| block.kind == "paragraph" && block.logical == "after\n")
    );
}

#[test]
fn invalid_giant_constructs_remain_source_visible_and_do_not_poison_tail() {
    let invalid_fence = format!("```{} ` suffix\n", "x".repeat(MIB));
    let token = CancellationToken::default();
    let mut fence = FenceJob::new(invalid_fence.as_bytes(), FenceMode::Open);
    assert_eq!(
        complete(|| fence.poll(invalid_fence.as_bytes(), DEFAULT_POLL_BYTES, &token)),
        None
    );

    let invalid_html = format!("<x a=\"{}\n", "v".repeat(MIB));
    let mut html = HtmlType7Job::new(invalid_html.as_bytes());
    assert!(!complete(|| html.poll(
        invalid_html.as_bytes(),
        DEFAULT_POLL_BYTES,
        &token
    )));

    for invalid in [invalid_fence, invalid_html] {
        let document = format!("{invalid}\nafter\n");
        let blocks = oracle_block_projection(&document, false);
        assert!(
            blocks
                .iter()
                .any(|block| block.kind == "paragraph" && block.logical == "after\n")
        );
    }
}
