//! Release-mode executable receipts for the Crop owned-adapter decision gate.

#[cfg(not(feature = "crop-research"))]
fn main() {
    panic!("re-run with --features crop-research");
}

#[cfg(feature = "crop-research")]
mod enabled {
    use std::collections::VecDeque;
    use std::env;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use flark_integrated_parser_slice::block::{BlockJob, BlockOutput, BlockStatus};
    use flark_integrated_parser_slice::crop_source::CropSnapshotLease;
    use flark_integrated_parser_slice::frontier::{LexerStatus, SharedLexer};
    use flark_integrated_parser_slice::inline_machine::{
        InlineMachine, InlineStatus, MAX_INLINE_COOPERATIVE_TRANSITIONS,
    };
    use flark_integrated_parser_slice::source::PersistentSource;

    const HISTORY: usize = 64;
    const BLOCK_FUEL: usize = 4_096;
    const LEXER_FUEL: usize = 4_096;
    const PATTERN: &str = "plain live markdown source with no delimiter runs and stable bytes. ";
    const DENSE_PATTERN: &str = "*a* ";

    pub fn main() {
        let mut arguments = env::args().skip(1);
        let mode = arguments.next().unwrap_or_else(|| "pipeline".to_owned());
        let backend = arguments.next().unwrap_or_else(|| "crop".to_owned());
        let mib = parse_or(arguments.next(), 10_usize);
        let edits = parse_or(arguments.next(), 1_000_usize);
        let requested = mib.checked_mul(1024 * 1024).expect("size overflow");
        let pattern = if env::var("FLARK_GATE_SHAPE").as_deref() == Ok("dense") {
            DENSE_PATTERN
        } else {
            PATTERN
        };
        match (mode.as_str(), backend.as_str()) {
            ("pipeline", "crop") => pipeline_crop(make_text(requested, pattern), edits),
            ("pipeline", "custom") => pipeline_custom(make_text(requested, pattern), edits),
            ("cancel", "crop") => cancel_crop(make_text(requested, pattern)),
            ("cancel", "custom") => cancel_custom(make_text(requested, pattern)),
            _ => panic!("usage: crop_owned_gate [pipeline|cancel] [crop|custom] [MiB] [edits]"),
        }
    }

    fn parse_or(value: Option<String>, default: usize) -> usize {
        value.map_or(default, |value| value.parse().expect("positive integer"))
    }

    fn make_text(bytes: usize, pattern: &str) -> String {
        let mut text = String::with_capacity(bytes);
        while text.len() + pattern.len() <= bytes {
            text.push_str(pattern);
        }
        text.extend(std::iter::repeat_n('a', bytes - text.len()));
        text
    }

    #[derive(Default)]
    struct PipelineReceipt {
        block: Duration,
        lexer: Duration,
        inline: Duration,
        total: Duration,
        block_polls: usize,
        lexer_polls: usize,
        inline_polls: usize,
        leaves: usize,
        spans: usize,
        digest: u64,
        source_bytes: usize,
        source_index_nodes: usize,
        source_chunk_loads: usize,
        source_chunk_bytes_copied: usize,
        source_fragment_nodes: usize,
        source_fragment_handles: usize,
        lexer_source_chunk_loads: usize,
        lexer_source_chunk_bytes_copied: usize,
        inline_source_chunk_loads: usize,
        inline_source_chunk_bytes_copied: usize,
    }

    fn finish_pipeline(mut block: BlockJob) -> PipelineReceipt {
        let total_started = Instant::now();
        let block_started = Instant::now();
        let mut block_polls = 0;
        loop {
            block_polls += 1;
            match block.poll(BLOCK_FUEL).status {
                BlockStatus::Pending => {}
                BlockStatus::Ready => break,
                BlockStatus::Failed => panic!("block failed: {:?}", block.error()),
            }
        }
        let block_elapsed = block_started.elapsed();
        let output = block.result().expect("ready block output").clone();
        let block_receipt = output.receipt();
        let mut receipt = PipelineReceipt {
            block: block_elapsed,
            block_polls,
            leaves: output.len(),
            source_bytes: block_receipt.source_bytes_inspected,
            source_index_nodes: block_receipt.source_index_nodes_examined,
            source_chunk_loads: block_receipt.source_chunk_loads,
            source_chunk_bytes_copied: block_receipt.source_chunk_bytes_copied,
            source_fragment_nodes: block_receipt.source_fragment_nodes_allocated,
            source_fragment_handles: block_receipt.source_fragment_handles_retained,
            ..PipelineReceipt::default()
        };
        drop(block);
        parse_leaves(&output, &mut receipt);
        receipt.total = total_started.elapsed();
        receipt
    }

    fn parse_leaves(output: &BlockOutput, receipt: &mut PipelineReceipt) {
        for leaf in output.leaves() {
            let lexer_started = Instant::now();
            let mut lexer = SharedLexer::new(&leaf.input);
            loop {
                receipt.lexer_polls += 1;
                if lexer.poll(LEXER_FUEL).status == LexerStatus::Ready {
                    break;
                }
            }
            let lexer_source = lexer.cursor_metrics();
            receipt.lexer_source_chunk_loads += lexer_source.source_chunk_loads;
            receipt.lexer_source_chunk_bytes_copied += lexer_source.source_chunk_bytes_copied;
            receipt.lexer += lexer_started.elapsed();
            let inline_started = Instant::now();
            let mut inline = InlineMachine::new(lexer.consumers().unwrap().inline);
            loop {
                receipt.inline_polls += 1;
                let poll = inline.poll_cooperative(MAX_INLINE_COOPERATIVE_TRANSITIONS);
                receipt.inline_source_chunk_loads += poll.telemetry_delta.source_chunk_loads;
                receipt.inline_source_chunk_bytes_copied +=
                    poll.telemetry_delta.source_chunk_bytes_copied;
                if poll.status == InlineStatus::Ready {
                    break;
                }
            }
            let output = inline.take_output().unwrap();
            receipt.spans += output.span_count();
            receipt.digest ^= output.digest();
            receipt.inline += inline_started.elapsed();
        }
    }

    fn pipeline_crop(text: String, edits: usize) {
        let bytes = text.len();
        let build_started = Instant::now();
        let source = CropSnapshotLease::from_text(&text);
        let build = build_started.elapsed();
        drop(text);
        let (source, edit) = edit_crop(source, edits);
        let root_identity = source.identity().0;
        let receipt = finish_pipeline(BlockJob::new_crop(source.clone()));
        let final_drop_started = Instant::now();
        drop(source);
        let final_drop = final_drop_started.elapsed();
        print_pipeline(
            "crop",
            bytes,
            build,
            &edit,
            final_drop,
            root_identity,
            &receipt,
        );
    }

    fn pipeline_custom(text: String, edits: usize) {
        let bytes = text.len();
        let build_started = Instant::now();
        let source = Arc::new(PersistentSource::from_text(&text));
        let build = build_started.elapsed();
        drop(text);
        let (source, edit) = edit_custom(source, edits);
        let root_identity = source.identity().0;
        let receipt = finish_pipeline(BlockJob::new(source.clone()));
        let final_drop_started = Instant::now();
        drop(source);
        let final_drop = final_drop_started.elapsed();
        print_pipeline(
            "custom",
            bytes,
            build,
            &edit,
            final_drop,
            root_identity,
            &receipt,
        );
    }

    struct EditReceipt {
        total: Duration,
        p50: Duration,
        p99: Duration,
        maximum: Duration,
        history_drop: Duration,
    }

    fn edit_crop(
        mut source: Arc<CropSnapshotLease>,
        edits: usize,
    ) -> (Arc<CropSnapshotLease>, EditReceipt) {
        let mut history = VecDeque::with_capacity(HISTORY);
        let mut timings = Vec::with_capacity(edits);
        let started = Instant::now();
        for index in 0..edits {
            let at = edit_offset(source.len_bytes(), index);
            let tick = Instant::now();
            let (next, _) = source.edit(at..at + 1, replacement(index)).unwrap();
            timings.push(tick.elapsed());
            history.push_back(source);
            source = next;
            if history.len() > HISTORY {
                history.pop_front();
            }
        }
        let total = started.elapsed();
        let history_drop_started = Instant::now();
        drop(history);
        let history_drop = history_drop_started.elapsed();
        (source, summarize_edits(timings, total, history_drop))
    }

    fn edit_custom(
        mut source: Arc<PersistentSource>,
        edits: usize,
    ) -> (Arc<PersistentSource>, EditReceipt) {
        let mut history = VecDeque::with_capacity(HISTORY);
        let mut timings = Vec::with_capacity(edits);
        let started = Instant::now();
        for index in 0..edits {
            let at = edit_offset(source.len_bytes(), index);
            let tick = Instant::now();
            let next = Arc::new(source.edit(at..at + 1, replacement(index)).unwrap().source);
            timings.push(tick.elapsed());
            history.push_back(source);
            source = next;
            if history.len() > HISTORY {
                history.pop_front();
            }
        }
        let total = started.elapsed();
        let history_drop_started = Instant::now();
        drop(history);
        let history_drop = history_drop_started.elapsed();
        (source, summarize_edits(timings, total, history_drop))
    }

    fn edit_offset(len: usize, index: usize) -> usize {
        if len <= 2 {
            return 0;
        }
        let mixed = index
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        1 + mixed % (len - 2)
    }

    const fn replacement(index: usize) -> &'static str {
        if index.is_multiple_of(2) {
            "x"
        } else {
            "y"
        }
    }

    fn summarize_edits(
        mut values: Vec<Duration>,
        total: Duration,
        history_drop: Duration,
    ) -> EditReceipt {
        values.sort_unstable();
        let percentile = |numerator: usize| {
            values
                .get(values.len().saturating_sub(1) * numerator / 100)
                .copied()
                .unwrap_or_default()
        };
        EditReceipt {
            total,
            p50: percentile(50),
            p99: percentile(99),
            maximum: values.last().copied().unwrap_or_default(),
            history_drop,
        }
    }

    fn print_pipeline(
        backend: &str,
        bytes: usize,
        build: Duration,
        edit: &EditReceipt,
        final_drop: Duration,
        root_identity: u64,
        receipt: &PipelineReceipt,
    ) {
        println!(
            "mode=pipeline backend={backend} bytes={bytes} root={root_identity} build_us={} edit_total_us={} edit_p50_ns={} edit_p99_ns={} edit_max_us={} history_drop_us={} block_us={} lexer_us={} inline_us={} total_us={} block_polls={} lexer_polls={} inline_polls={} leaves={} spans={} digest={:016x} source_bytes={} source_index_nodes={} block_source_chunk_loads={} block_source_chunk_bytes_copied={} lexer_source_chunk_loads={} lexer_source_chunk_bytes_copied={} inline_source_chunk_loads={} inline_source_chunk_bytes_copied={} source_fragment_nodes={} source_fragment_handles={} final_drop_us={}",
            build.as_micros(),
            edit.total.as_micros(),
            edit.p50.as_nanos(),
            edit.p99.as_nanos(),
            edit.maximum.as_micros(),
            edit.history_drop.as_micros(),
            receipt.block.as_micros(),
            receipt.lexer.as_micros(),
            receipt.inline.as_micros(),
            receipt.total.as_micros(),
            receipt.block_polls,
            receipt.lexer_polls,
            receipt.inline_polls,
            receipt.leaves,
            receipt.spans,
            receipt.digest,
            receipt.source_bytes,
            receipt.source_index_nodes,
            receipt.source_chunk_loads,
            receipt.source_chunk_bytes_copied,
            receipt.lexer_source_chunk_loads,
            receipt.lexer_source_chunk_bytes_copied,
            receipt.inline_source_chunk_loads,
            receipt.inline_source_chunk_bytes_copied,
            receipt.source_fragment_nodes,
            receipt.source_fragment_handles,
            final_drop.as_micros(),
        );
    }

    fn cancel_crop(text: String) {
        let bytes = text.len();
        let source = CropSnapshotLease::from_text(&text);
        drop(text);
        let mut job = BlockJob::new_crop(source.clone());
        for _ in 0..256 {
            assert_eq!(job.poll(BLOCK_FUEL).status, BlockStatus::Pending);
        }
        let inspected = job.receipt().source_bytes_inspected;
        let hot_started = Instant::now();
        drop(job);
        let hot = hot_started.elapsed();
        let final_started = Instant::now();
        drop(source);
        let final_drop = final_started.elapsed();
        println!(
            "mode=cancel backend=crop bytes={bytes} inspected={inspected} hot_cancel_us={} final_drop_us={}",
            hot.as_micros(),
            final_drop.as_micros()
        );
    }

    fn cancel_custom(text: String) {
        let bytes = text.len();
        let source = Arc::new(PersistentSource::from_text(&text));
        drop(text);
        let mut job = BlockJob::new(source.clone());
        for _ in 0..256 {
            assert_eq!(job.poll(BLOCK_FUEL).status, BlockStatus::Pending);
        }
        let inspected = job.receipt().source_bytes_inspected;
        let hot_started = Instant::now();
        drop(job);
        let hot = hot_started.elapsed();
        let final_started = Instant::now();
        drop(source);
        let final_drop = final_started.elapsed();
        println!(
            "mode=cancel backend=custom bytes={bytes} inspected={inspected} hot_cancel_us={} final_drop_us={}",
            hot.as_micros(),
            final_drop.as_micros()
        );
    }
}

#[cfg(feature = "crop-research")]
fn main() {
    enabled::main();
}
