//! Release-mode wall-time probe for cooperative parser slice boundaries.
//!
//! This is diagnostic evidence, not an acceptance benchmark. It deliberately
//! times every public poll so a nominal fuel counter cannot hide a long atomic
//! operation. Run each shape in a fresh process, for example:
//!
//! ```text
//! cargo run --release --bin slice_latency -- plain 10485760 1 drop cooperative
//! cargo run --release --bin slice_latency -- inline 1048576 4096 retain exact
//! ```
//!
//! The fourth argument is `drop` (the parser-only default) or `retain` for an
//! end-to-end ingest-retention receipt. The fifth is `cooperative` (the
//! production-model default) or `exact` for the multidimensional admission
//! baseline. Both modes return the same actual work receipt and output.

use std::env;
use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

use flark_integrated_parser_slice::block::{BlockJob, BlockStatus};
use flark_integrated_parser_slice::frontier::{LexerStatus, SharedLexer};
use flark_integrated_parser_slice::inline_machine::{InlineMachine, InlineStatus, InlineWork};
use flark_integrated_parser_slice::source::PersistentSource;

fn main() {
    let mut arguments = env::args().skip(1);
    let shape = arguments.next().unwrap_or_else(|| "inline".to_owned());
    let bytes = arguments
        .next()
        .map_or(1024 * 1024, |value| value.parse().expect("byte count"));
    let fuel = arguments
        .next()
        .map_or(4096, |value| value.parse().expect("fuel"));
    let retain_generator = arguments.next().is_some_and(|value| value == "retain");
    let inline_mode = arguments.next().unwrap_or_else(|| "cooperative".to_owned());
    assert!(matches!(inline_mode.as_str(), "cooperative" | "exact"));
    assert!(fuel > 0 && fuel <= 4096);

    let text = make_shape(&shape, bytes);
    let actual_bytes = text.len();
    let source_started = Instant::now();
    let source = Arc::new(PersistentSource::from_text(black_box(&text)));
    let source_elapsed = source_started.elapsed();
    let retained_generator = retain_generator.then_some(text);

    let block_started = Instant::now();
    let mut block = BlockJob::new(source);
    let block_construct = block_started.elapsed();
    let mut block_polls = Timings::default();
    loop {
        let started = Instant::now();
        let poll = block.poll(fuel);
        block_polls.push(started.elapsed());
        match poll.status {
            BlockStatus::Pending => {}
            BlockStatus::Ready => break,
            BlockStatus::Failed => panic!("block failed: {:?}", block.error()),
        }
    }
    let output = block.result().expect("ready block result");
    let mut lexer_construct = Duration::ZERO;
    let mut lexer_polls = Timings::default();
    let mut inline_construct = Duration::ZERO;
    let mut inline_polls = Timings::default();
    let mut slowest_inline_elapsed = Duration::ZERO;
    let mut slowest_inline_work = InlineWork::default();
    let mut spans = 0;
    let mut processed_leaves = 0;
    for leaf in output.leaves() {
        processed_leaves += 1;
        let started = Instant::now();
        let mut lexer = SharedLexer::new(&leaf.input);
        lexer_construct += started.elapsed();
        loop {
            let started = Instant::now();
            let poll = lexer.poll(fuel);
            lexer_polls.push(started.elapsed());
            if poll.status == LexerStatus::Ready {
                break;
            }
        }
        let consumers = lexer.consumers().expect("ready lexer consumers");
        let started = Instant::now();
        let mut inline = InlineMachine::new(consumers.inline);
        inline_construct += started.elapsed();
        loop {
            let started = Instant::now();
            let poll = if inline_mode == "cooperative" {
                inline.poll_cooperative(fuel)
            } else {
                inline.poll(InlineWork::uniform(fuel))
            };
            let elapsed = started.elapsed();
            if elapsed > slowest_inline_elapsed {
                slowest_inline_elapsed = elapsed;
                slowest_inline_work = poll.delta;
            }
            inline_polls.push(elapsed);
            if poll.status == InlineStatus::Ready {
                break;
            }
            assert!(poll.delta.transitions > 0, "uniform permit must progress");
        }
        spans += inline.output().expect("ready inline output").span_count();
    }
    black_box(&retained_generator);

    println!(
        "shape={shape} requested_bytes={bytes} actual_bytes={actual_bytes} fuel={fuel} generator_retained={} inline_mode={inline_mode} source_us={} block_construct_us={} block={} leaves={} processed_leaves={processed_leaves} lexer_construct_us={} lexer={} inline_construct_us={} inline={} slowest_inline_ns={} slowest_inline_work={} spans={spans}",
        retained_generator.is_some(),
        micros(source_elapsed),
        micros(block_construct),
        block_polls.summary(),
        output.len(),
        micros(lexer_construct),
        lexer_polls.summary(),
        micros(inline_construct),
        inline_polls.summary(),
        nanos(slowest_inline_elapsed),
        inline_work_summary(&slowest_inline_work),
    );
}

fn inline_work_summary(work: &InlineWork) -> String {
    format!(
        "transitions:{},allocations:{},allocated_bytes:{},reclaims:{},reclaimed_bytes:{},copy_bytes:{},hash_bytes:{},source_bytes:{}",
        work.transitions,
        work.page_allocations,
        work.allocated_bytes,
        work.page_reclaims,
        work.reclaimed_bytes,
        work.copy_bytes,
        work.hash_bytes,
        work.source_logical_bytes + work.source_excluded_bytes,
    )
}

fn make_shape(shape: &str, bytes: usize) -> String {
    let pattern = match shape {
        "plain" => "a",
        "inline" => "*a* ",
        "unmatched" => "_a ",
        "paragraphs" => "a\n\n",
        "softbreaks" => "a\n",
        "quotes" => "> > > > > > > > a\n",
        _ => panic!("unknown shape {shape:?}"),
    };
    let mut result = String::with_capacity(bytes);
    while result.len() + pattern.len() <= bytes {
        result.push_str(pattern);
    }
    if result.len() < bytes {
        result.push_str(&pattern[..bytes - result.len()]);
    }
    result
}

struct Timings {
    buckets: Box<[u64]>,
    count: u64,
    total: Duration,
    max: Duration,
}

const SUB_100_US_BUCKETS: usize = 1_000;
const SUB_10_MS_BUCKETS: usize = 990;
const SUB_1_S_BUCKETS: usize = 990;
const TIMING_BUCKETS: usize = SUB_100_US_BUCKETS + SUB_10_MS_BUCKETS + SUB_1_S_BUCKETS + 1;

impl Default for Timings {
    fn default() -> Self {
        Self {
            buckets: vec![0; TIMING_BUCKETS].into_boxed_slice(),
            count: 0,
            total: Duration::ZERO,
            max: Duration::ZERO,
        }
    }
}

impl Timings {
    fn push(&mut self, value: Duration) {
        self.buckets[timing_bucket(value)] += 1;
        self.count += 1;
        self.total += value;
        self.max = self.max.max(value);
    }

    fn summary(self) -> String {
        format!(
            "polls={},total_us={},p50_ns_le={},p95_ns_le={},p99_ns_le={},p999_ns_le={},max_ns={}",
            self.count,
            micros(self.total),
            self.percentile_upper_ns(500),
            self.percentile_upper_ns(950),
            self.percentile_upper_ns(990),
            self.percentile_upper_ns(999),
            nanos(self.max),
        )
    }

    fn percentile_upper_ns(&self, permille: u64) -> u128 {
        if self.count == 0 {
            return 0;
        }
        let target = self.count.saturating_mul(permille).div_ceil(1_000);
        let mut seen = 0_u64;
        for (index, count) in self.buckets.iter().copied().enumerate() {
            seen += count;
            if seen >= target {
                return bucket_upper_ns(index);
            }
        }
        nanos(self.max)
    }
}

fn timing_bucket(value: Duration) -> usize {
    let nanos = nanos(value);
    if nanos < 100_000 {
        return usize::try_from(nanos / 100).unwrap_or(SUB_100_US_BUCKETS - 1);
    }
    if nanos < 10_000_000 {
        return SUB_100_US_BUCKETS
            + usize::try_from((nanos - 100_000) / 10_000).unwrap_or(SUB_10_MS_BUCKETS - 1);
    }
    if nanos < 1_000_000_000 {
        return SUB_100_US_BUCKETS
            + SUB_10_MS_BUCKETS
            + usize::try_from((nanos - 10_000_000) / 1_000_000).unwrap_or(SUB_1_S_BUCKETS - 1);
    }
    TIMING_BUCKETS - 1
}

fn bucket_upper_ns(index: usize) -> u128 {
    if index < SUB_100_US_BUCKETS {
        return (index as u128 + 1) * 100;
    }
    if index < SUB_100_US_BUCKETS + SUB_10_MS_BUCKETS {
        return 100_000 + (index as u128 - SUB_100_US_BUCKETS as u128 + 1) * 10_000;
    }
    if index < TIMING_BUCKETS - 1 {
        return 10_000_000
            + (index as u128 - SUB_100_US_BUCKETS as u128 - SUB_10_MS_BUCKETS as u128 + 1)
                * 1_000_000;
    }
    u128::MAX
}

fn micros(value: Duration) -> u128 {
    value.as_micros()
}

fn nanos(value: Duration) -> u128 {
    value.as_nanos()
}
