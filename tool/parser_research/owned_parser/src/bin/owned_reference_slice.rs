use std::time::Instant;

use flark_owned_parser_trial::{
    AdvanceStatus, CancelFlag, EditRequest, RevisionedDocument, WorkBudget,
};

fn main() {
    let mut source = String::from("[shared]: /first\n\n");
    for index in 0..20_000 {
        source.push_str(&format!("paragraph {index} uses [shared]\n\n"));
    }
    let offset = source.find("first").unwrap();
    let started = Instant::now();
    let mut document = RevisionedDocument::new(&source);
    let initial_us = started.elapsed().as_micros();
    let mut samples = Vec::new();
    for iteration in 0..1_000 {
        let replacement = if iteration % 2 == 0 { "prime" } else { "first" };
        let mut edit = document
            .begin_edit(EditRequest {
                base_revision: document.revision(),
                before_hash32: document.hash32(),
                start_utf8: offset,
                end_utf8: offset + 5,
                replacement: replacement.to_owned(),
            })
            .unwrap();
        let started = Instant::now();
        loop {
            match edit.advance(WorkBudget::new(2, 128), &CancelFlag::default()) {
                AdvanceStatus::Pending => {}
                AdvanceStatus::Converged => break,
                AdvanceStatus::Cancelled => unreachable!(),
            }
        }
        let delta = document.adopt(edit.into_result().unwrap()).unwrap();
        assert_eq!(delta.references.invalidated_lookup_records, 20_000);
        assert!(delta.reparsed_bytes < 80);
        samples.push(started.elapsed().as_micros());
    }
    samples.sort_unstable();
    println!(
        "owned_reference_slice bytes={} lookups={} initial_us={} apply_p50_us={} apply_p95_us={} apply_p99_us={} max_us={}",
        document.len_utf8(),
        document.reference_lookup_count("shared"),
        initial_us,
        percentile(&samples, 50),
        percentile(&samples, 95),
        percentile(&samples, 99),
        samples.last().unwrap(),
    );
}

fn percentile(values: &[u128], percentile: usize) -> u128 {
    values[(values.len() - 1) * percentile / 100]
}
