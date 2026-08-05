use std::hint::black_box;
use std::time::Instant;

use comrak::inline_fragment::{
    EMPTY_REFERENCE_SNAPSHOT, InlineFragmentRequest, InlineInputKind, InlineProfile,
    parse_inline_fragment,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let bytes: usize = args
        .next()
        .unwrap_or_else(|| "8192".into())
        .parse()
        .unwrap();
    let shape = args.next().unwrap_or_else(|| "dense".into());
    let iterations: usize = args
        .next()
        .unwrap_or_else(|| "2000".into())
        .parse()
        .unwrap();
    let source = generate(bytes, &shape);

    for _ in 0..100 {
        black_box(run(&source));
    }
    let mut samples = Vec::with_capacity(iterations);
    let mut receipt = (0, 0, false);
    for _ in 0..iterations {
        let started = Instant::now();
        receipt = run(&source);
        samples.push(started.elapsed().as_nanos() as u64);
    }
    samples.sort_unstable();
    println!(
        "backend=native shape={shape} bytes={bytes} iterations={iterations} p50_ns={} p99_ns={} max_ns={} facts={} output_bytes={} rejected={} max_rss_bytes={}",
        percentile(&samples, 50),
        percentile(&samples, 99),
        samples.last().copied().unwrap_or(0),
        receipt.0,
        receipt.1,
        receipt.2,
        max_rss_bytes(),
    );
}

fn run(source: &str) -> (usize, usize, bool) {
    let request = InlineFragmentRequest {
        logical: source,
        leaf_id: 1,
        kind: InlineInputKind::Paragraph,
        profile: InlineProfile::Gfm,
        reference_snapshot: &EMPTY_REFERENCE_SNAPSHOT,
        revision: 1,
        expected_revision: 1,
    };
    match parse_inline_fragment(request) {
        Ok(fragment) => (
            fragment.facts.len() + fragment.projection_facts.len(),
            fragment.output_bytes(),
            false,
        ),
        Err(_) => (0, 0, true),
    }
}

fn generate(bytes: usize, shape: &str) -> String {
    let atom = match shape {
        "unmatched" => "***[[[```~~~__ ",
        "links" => "[link](https://example.com/a_(b)) ![i](p.png) ",
        "plain" => "An ordinary prose sentence without markup. ",
        "ordinary" => "An *ordinary* paragraph with **strong words**, `code`, and [a link](u). ",
        _ => "**bold** *em* ` code ` ~~strike~~ &copy; ",
    };
    let mut source = String::with_capacity(bytes);
    while source.len() + atom.len() <= bytes {
        source.push_str(atom);
    }
    source.push_str(&"x".repeat(bytes - source.len()));
    source
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    samples[(samples.len().saturating_sub(1) * percentile) / 100]
}

#[cfg(target_os = "macos")]
fn max_rss_bytes() -> i64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage initializes the pointed-to rusage on success.
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if result == 0 {
        // SAFETY: successful getrusage initialized `usage`.
        unsafe { usage.assume_init().ru_maxrss }
    } else {
        -1
    }
}

#[cfg(not(target_os = "macos"))]
fn max_rss_bytes() -> i64 {
    -1
}
