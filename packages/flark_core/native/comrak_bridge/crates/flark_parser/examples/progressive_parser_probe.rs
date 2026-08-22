use std::time::Instant;

use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, OpeningSourceStore, SourceRevision,
    SOURCE_SEED_PAGE_MAX_UTF16,
};
use flark_parser::{
    build_m11_progressive_compact_probe, build_m11_progressive_open_probe,
    M11CompactViewportProbe, M11ProgressiveOpenFeed,
};

const ADMISSION_GRANT_BYTES: usize = 256 * 1024;
const BLOCK: &str = "Ordinary paragraph with **bold** text and plain words.\n\n";

fn fixture(size_mib: usize) -> (String, Vec<usize>) {
    let target = size_mib * 1024 * 1024;
    let mut source = String::with_capacity(target + BLOCK.len());
    let mut frontiers = Vec::new();
    let mut blocks = 0_usize;
    let mut next_grant = ADMISSION_GRANT_BYTES;
    while source.len() < target {
        source.push_str(BLOCK);
        blocks += 1;
        if blocks == 8 || blocks == 40 || source.len() >= next_grant {
            if frontiers.last().copied() != Some(source.len()) {
                frontiers.push(source.len());
            }
            while next_grant <= source.len() {
                next_grant += ADMISSION_GRANT_BYTES;
            }
        }
    }
    if frontiers.last().copied() != Some(source.len()) {
        frontiers.push(source.len());
    }
    (source, frontiers)
}

fn release(mut probe: M11CompactViewportProbe, runtime: &mut DocumentRuntime) {
    probe.begin_release(runtime).expect("begin probe release");
    while !probe
        .poll_release(runtime, 4_096)
        .expect("poll probe release")
    {}
}

fn run(size_mib: usize) {
    let (source, frontiers) = fixture(size_mib);
    let runtime_started = Instant::now();
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("document runtime");
    let runtime_open = runtime_started.elapsed();
    let probe = build_m11_progressive_compact_probe(&mut runtime, 1, &frontiers)
        .expect("progressive parser probe");
    let first_structural = probe.first_structural_slice_elapsed();
    let early = probe
        .early_certification_elapsed()
        .expect("ordinary fixture certifies before EOF");
    let complete = probe.complete_elapsed();
    let admitted = probe.first_structural_slice_admitted_bytes();
    let starvation_count = probe.starvation_count();
    let (early_viewport, final_viewport) = probe.into_viewports();
    release(
        early_viewport.expect("ordinary early viewport"),
        &mut runtime,
    );
    release(final_viewport, &mut runtime);
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(4_096).expect("poll close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);

    println!(
        "{{\"requested_mib\":{},\"source_bytes\":{},\"runtime_open_ms\":{:.3},\"first_structural_ms\":{:.3},\"early_certified_ms\":{:.3},\"first_slice_admitted_bytes\":{},\"complete_ms\":{:.3},\"starvations\":{}}}",
        size_mib,
        source.len(),
        runtime_open.as_secs_f64() * 1000.0,
        first_structural.as_secs_f64() * 1000.0,
        early.as_secs_f64() * 1000.0,
        admitted,
        complete.as_secs_f64() * 1000.0,
        starvation_count,
    );
}

/// The same fixture admitted for real through the opening store in 8 Ki
/// UTF-16 pages, so the timing includes admission, append proofs, runtime
/// adoption, and the starved-parser resume protocol.
fn run_open(size_mib: usize) {
    let (source, _) = fixture(size_mib);
    assert!(source.is_ascii(), "byte offsets double as UTF-16 offsets");
    let started = Instant::now();
    let mut store =
        OpeningSourceStore::new(SourceRevision::new(1), None).expect("opening store");
    let mut runtime = DocumentRuntime::from_opening_snapshot(
        store.snapshot(),
        DocumentRuntimeConfig::default(),
    )
    .expect("opening runtime");
    let admitted = store.version();
    let mut offset = 0;
    let probe = build_m11_progressive_open_probe(&mut runtime, 1, &mut store, admitted, |store| {
        if offset == source.len() {
            return Ok(M11ProgressiveOpenFeed::Exhausted);
        }
        let end = source.len().min(offset + SOURCE_SEED_PAGE_MAX_UTF16);
        let version = store.version();
        store
            .append_page(version, offset..end, &source[offset..end])
            .expect("transport page append");
        offset = end;
        Ok(M11ProgressiveOpenFeed::Appended)
    })
    .expect("opening-store progressive probe");
    let complete = started.elapsed();
    let early = probe.early_certification_elapsed();
    let first_structural = probe.first_structural_slice_elapsed();
    let starvation_count = probe.starvation_count();
    let (early_viewport, final_viewport) = probe.into_viewports();
    if let Some(early_viewport) = early_viewport {
        release(early_viewport, &mut runtime);
    }
    release(final_viewport, &mut runtime);
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(4_096).expect("poll close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);

    println!(
        "{{\"path\":\"open_store\",\"requested_mib\":{},\"source_bytes\":{},\"first_structural_ms\":{:.3},\"early_certified_ms\":{},\"complete_ms\":{:.3},\"starvations\":{}}}",
        size_mib,
        source.len(),
        first_structural.as_secs_f64() * 1000.0,
        early
            .map_or_else(|| "null".to_string(), |value| format!("{:.3}", value.as_secs_f64() * 1000.0)),
        complete.as_secs_f64() * 1000.0,
        starvation_count,
    );
}

fn main() {
    let sizes = std::env::args()
        .skip(1)
        .map(|argument| argument.parse::<usize>().expect("size is an integer MiB"))
        .collect::<Vec<_>>();
    for size in if sizes.is_empty() {
        vec![1, 10, 40]
    } else {
        sizes
    } {
        run(size);
        run_open(size);
    }
}
