use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Instant;

use flark_bounded_comrak_challenger::{
    build_bounded_leaf_index, diff_regions, gfm_options, is_certified_payload_edit,
    RegionDisposition,
};

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let shape = args.get(1).map(String::as_str).unwrap_or("paragraph");
    let mib = args
        .get(2)
        .map(|value| value.parse::<usize>().expect("MiB must be an integer"))
        .unwrap_or(1);
    let cap = args
        .get(3)
        .map(|value| value.parse::<usize>().expect("cap must be an integer"))
        .unwrap_or(65_536);
    let target = mib * 1_048_576;
    let (source, edit_offset) = generate(shape, target);
    let source: Arc<str> = source.into();
    let options = gfm_options();

    let started = Instant::now();
    let baseline = build_bounded_leaf_index(Arc::clone(&source), cap, &options);
    let initial_us = started.elapsed().as_micros();

    let mut edited = source.to_string();
    let replacement = if edited.as_bytes()[edit_offset] == b'a' {
        "b"
    } else {
        "a"
    };
    edited.replace_range(edit_offset..edit_offset + 1, replacement);
    let edited: Arc<str> = edited.into();
    let edit_started = Instant::now();
    let after = build_bounded_leaf_index(edited, cap, &options);
    let full_edit_us = edit_started.elapsed().as_micros();
    let delta = diff_regions(&baseline.regions, &after.regions);

    let active_region = baseline
        .region_containing(edit_offset)
        .expect("generated edit must be in a semantic region");
    let local_started = Instant::now();
    let local_mode = match active_region.disposition {
        RegionDisposition::Delegated { .. } => {
            let edited_fragment: Arc<str> = after.source()[active_region.range.clone()]
                .to_owned()
                .into();
            let local = build_bounded_leaf_index(edited_fragment, cap, &options);
            std::hint::black_box(local);
            "delegate-region"
        }
        RegionDisposition::SourceBackedRaw
        | RegionDisposition::OpaqueOverCap
        | RegionDisposition::OpaqueUnsupported(_) => {
            let certified = is_certified_payload_edit(
                baseline.source(),
                edit_offset..edit_offset + 1,
                replacement,
            );
            assert!(certified, "generated edit must be a certified payload edit");
            std::hint::black_box(certified);
            if matches!(
                active_region.disposition,
                RegionDisposition::SourceBackedRaw
            ) {
                "source-backed-raw"
            } else {
                "retain-opaque"
            }
        }
    };
    let local_edit_ns = local_started.elapsed().as_nanos();

    println!(
        "bounded shape={shape} target_mib={mib} source_bytes={} cap_bytes={cap} initial_us={initial_us} full_edit_us={full_edit_us} local_edit_mode={local_mode} local_edit_ns={local_edit_ns} regions={} delegated={} delegated_bytes={} delegated_nodes={} source_backed_raw={} source_backed_raw_bytes={} opaque={} opaque_bytes={} work={} inspected={} max_poll_bytes={} lines={} premature_comrak_calls={} delta_removed={} delta_inserted={} delta_changed_source_bytes={} delta_protocol_bytes={}",
        source.len(),
        baseline.regions.len(),
        baseline.metrics.delegated_regions,
        baseline.metrics.delegated_source_bytes,
        baseline.metrics.delegated_ast_nodes,
        baseline.metrics.source_backed_raw_regions,
        baseline.metrics.source_backed_raw_bytes,
        baseline.metrics.opaque_regions,
        baseline.metrics.opaque_source_bytes,
        baseline.metrics.block_work_units,
        baseline.metrics.source_bytes_inspected,
        baseline.metrics.maximum_bytes_per_poll,
        baseline.metrics.completed_lines,
        baseline.metrics.premature_comrak_calls,
        delta.removed_regions,
        delta.inserted_regions,
        delta.changed_source_bytes,
        delta.estimated_protocol_bytes,
    );
}

fn generate(shape: &str, target: usize) -> (String, usize) {
    let mut source = String::with_capacity(target + 4_096);
    source.push_str("prefix paragraph\n\n");
    let giant_start = source.len();
    match shape {
        "many-small" => {
            let mut block = 0usize;
            while source.len() < target {
                write!(source, "ordinary block {block} has **strong** words ").unwrap();
                for _ in 0..128 {
                    source.push_str("and plain payload that stays comfortably bounded ");
                }
                source.push('\n');
                source.push('\n');
                block += 1;
            }
        }
        "paragraph" => {
            while source.len() < target {
                source.push_str("ordinary words with **strong** emphasis and plain payload ");
            }
            source.push_str("\n\n");
        }
        "fence" => {
            source.push_str("```text\n");
            while source.len() < target {
                source.push_str("code payload with no closing fence marker on this line\n");
            }
            source.push_str("```\n\n");
        }
        "list" => {
            let mut item = 0usize;
            while source.len() < target {
                writeln!(
                    source,
                    "- item {item} has **strong** words and ordinary payload"
                )
                .unwrap();
                item += 1;
            }
            source.push('\n');
        }
        "table" => {
            source.push_str("| name | value |\n| --- | ---: |\n");
            let mut row = 0usize;
            while source.len() < target {
                writeln!(source, "| ordinary row {row} | **strong** payload {row} |").unwrap();
                row += 1;
            }
            source.push('\n');
        }
        other => panic!("unknown shape {other:?}"),
    }
    source.push_str("suffix paragraph remains separately renderable\n");

    let edit_offset = source[giant_start..]
        .find("payload")
        .map(|relative| giant_start + relative + 4)
        .or_else(|| {
            source[giant_start..]
                .find("ordinary")
                .map(|relative| giant_start + relative + 4)
        })
        .unwrap_or(giant_start + 4);
    (source, edit_offset)
}
