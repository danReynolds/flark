use std::env;
use std::time::Instant;

use flark_comrak_derived_core_probe::commitment_spine::{CommitmentFact, CommitmentSpine};

fn main() {
    let args = env::args().collect::<Vec<_>>();
    let shape = args.get(1).map_or("list", String::as_str);
    let mebibytes = args
        .get(2)
        .map_or(10, |value| value.parse::<usize>().expect("MiB"));
    let fuel = args
        .get(3)
        .map_or(4_096, |value| value.parse::<usize>().expect("fuel"));
    let target = mebibytes * 1024 * 1024;
    let source = generate(shape, target);
    let started = Instant::now();
    let mut machine = CommitmentSpine::new(source);
    let mut polls = 0usize;
    let mut work = 0usize;
    let mut lines = 0usize;
    let mut facts = 0usize;
    let mut opaque = 0usize;
    while !machine.is_complete() {
        let report = machine.advance(fuel);
        polls += 1;
        work += report.work_units;
        lines += report.completed_lines;
        for fact in machine.take_facts() {
            facts += 1;
            opaque += usize::from(matches!(fact, CommitmentFact::Opaque { .. }));
        }
    }
    machine.advance(fuel);
    for fact in machine.take_facts() {
        facts += 1;
        opaque += usize::from(matches!(fact, CommitmentFact::Opaque { .. }));
    }
    println!(
        "shape={shape} bytes={} lines={lines} fuel={fuel} polls={polls} work={work} facts={facts} opaque={opaque} max_atomic={} elapsed_us={}",
        machine.source().len(),
        machine.maximum_atomic_classification_bytes(),
        started.elapsed().as_micros(),
    );
}

fn generate(shape: &str, target: usize) -> String {
    let atom = match shape {
        "list" => "- one\n",
        "loose-list" => "- one\n\n  second block\n",
        "table" => "head | value\n--- | ---\nrow | value\n",
        "html" => "<!-- x -->\nparagraph\n\n",
        "refs" => "[label]: /url \"title\"\n\n",
        "giant-table" => return format!("| {} |\n", "x".repeat(target)),
        "giant-html" => return format!("<script>{}</script>\n", "x".repeat(target)),
        "giant-ref" => return format!("[label]: /{}\n", "x".repeat(target)),
        other => panic!("unknown shape {other}"),
    };
    let mut source = String::with_capacity(target + atom.len());
    while source.len() < target {
        source.push_str(atom);
    }
    source
}
