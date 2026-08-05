//! Shows the scheduling boundary of stock Comrak block work. The cancellation
//! request is intentionally external: Comrak exposes no poll/cancel input and
//! returns only after the current whole-document parse has completed.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use comrak::{parse_document, Arena, Options};

const PAYLOAD_BYTES: usize = 10 * 1024 * 1024;

fn main() {
    for (name, source) in [
        ("fence", giant_fence()),
        ("html", giant_html()),
        ("table_row", giant_table_row()),
    ] {
        probe(name, source);
    }
}

fn probe(name: &'static str, source: String) {
    let (sender, receiver) = mpsc::sync_channel(1);
    let started = Instant::now();
    thread::spawn(move || {
        let mut options = Options::default();
        options.extension.table = true;
        let arena = Arena::new();
        let parse_started = Instant::now();
        let root = parse_document(&arena, &source, &options);
        let nodes = root.descendants().count();
        sender
            .send((parse_started.elapsed(), nodes))
            .expect("receiver remains live");
    });

    thread::sleep(Duration::from_millis(1));
    let cancel_requested = Instant::now();
    let (parse_elapsed, nodes) = receiver.recv().expect("worker returns");
    println!(
        "shape={name} bytes={PAYLOAD_BYTES} cancel_request_us={} return_after_cancel_us={} parse_us={} nodes={nodes}",
        cancel_requested.duration_since(started).as_micros(),
        cancel_requested.elapsed().as_micros(),
        parse_elapsed.as_micros(),
    );
}

fn giant_fence() -> String {
    let mut source = String::with_capacity(PAYLOAD_BYTES + 10);
    source.push_str("```\n");
    source.extend(std::iter::repeat_n('x', PAYLOAD_BYTES));
    source.push_str("\n```\n");
    source
}

fn giant_html() -> String {
    let mut source = String::with_capacity(PAYLOAD_BYTES + 32);
    source.push_str("<script>");
    source.extend(std::iter::repeat_n('x', PAYLOAD_BYTES));
    source.push_str("</script>\n");
    source
}

fn giant_table_row() -> String {
    let mut source = String::with_capacity(PAYLOAD_BYTES + 32);
    source.push_str("a | b\n-- | --\n|");
    while source.len() < PAYLOAD_BYTES {
        source.push_str("x|");
    }
    source.push('\n');
    source
}
