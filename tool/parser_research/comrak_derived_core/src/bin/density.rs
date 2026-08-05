use std::mem::size_of;
use std::time::Instant;

use flark_comrak_derived_core_probe::{
    BlockEvent, DerivedBlockMachine, LineChunk, LineRecord, RestartState,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let lines = args
        .next()
        .as_deref()
        .unwrap_or("1000000")
        .parse::<usize>()
        .expect("line count");
    let mode = args.next().unwrap_or_else(|| "retain".to_owned());
    assert!(matches!(mode.as_str(), "retain" | "drain"));

    let source = "a\n".repeat(lines);
    let source_bytes = source.len();
    let started = Instant::now();
    let mut machine = DerivedBlockMachine::new(source);
    let mut emitted = 0usize;
    while !machine.is_complete() {
        machine.advance(16_384);
        if mode == "drain" {
            emitted += machine.take_records().len();
        }
    }
    emitted += machine.records().len();

    println!(
        "density mode={mode} source_bytes={source_bytes} lines={lines} emitted={emitted} retained={} elapsed_ms={} sizeof_record={} sizeof_chunk={} sizeof_event={} sizeof_restart={}",
        machine.records().len(),
        started.elapsed().as_millis(),
        size_of::<LineRecord>(),
        size_of::<LineChunk>(),
        size_of::<BlockEvent>(),
        size_of::<RestartState>(),
    );
}
