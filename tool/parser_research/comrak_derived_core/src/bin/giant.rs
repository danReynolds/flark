use std::time::Instant;

use flark_comrak_derived_core_probe::DerivedBlockMachine;

fn run(name: &str, source: String) {
    let source_bytes = source.len();
    let started = Instant::now();
    let mut machine = DerivedBlockMachine::new(source);
    let mut polls = 0usize;
    let mut work = 0usize;
    let mut bytes = 0usize;
    let mut maximum_bytes = 0usize;
    while !machine.is_complete() {
        let report = machine.advance(4_096);
        polls += 1;
        work += report.work_units;
        bytes += report.bytes_inspected;
        maximum_bytes = maximum_bytes.max(report.bytes_inspected);
    }
    println!(
        "giant name={name} source_bytes={source_bytes} polls={polls} work={work} inspected={bytes} max_inspected_per_poll={maximum_bytes} elapsed_ms={}",
        started.elapsed().as_millis(),
    );
}

fn main() {
    run("paragraph", format!("{}\n", "a".repeat(10_000_000)));
    run("setext", format!("alpha\n{}\n", "=".repeat(10_000_000)));
}
