use std::env;
use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

use flark_v3_runtime_slice::{CropSnapshotLease, SourceRevision, SourceStore};

#[derive(Clone, Copy, Debug, Default)]
struct Sample {
    old_root_build: Duration,
    new_root_build: Duration,
    admission: Duration,
    deferred_old_drop: Duration,
    new_root_drop: Duration,
}

#[derive(Clone, Copy, Debug)]
enum Mode {
    Direct,
    Retained,
}

impl Mode {
    const fn label(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Retained => "retained",
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(clippy::struct_field_names)] // Unit suffix keeps probe output fields unambiguous.
struct DurationStats {
    p50_ns: u128,
    p95_ns: u128,
    max_ns: u128,
}

#[derive(Clone, Copy, Debug, Default)]
struct LineageSample {
    fill: Duration,
    snapshot: Duration,
    overwrite: Duration,
    historical_drop: Duration,
    snapshot_nodes: usize,
}

impl DurationStats {
    fn from_samples(samples: impl Iterator<Item = Duration>) -> Self {
        let mut values = samples
            .map(|duration| duration.as_nanos())
            .collect::<Vec<_>>();
        values.sort_unstable();
        assert!(
            !values.is_empty(),
            "at least one measured sample is required"
        );
        let p50 = percentile_index(values.len(), 50);
        let p95 = percentile_index(values.len(), 95);
        Self {
            p50_ns: values[p50],
            p95_ns: values[p95],
            max_ns: *values.last().expect("nonempty timing sample"),
        }
    }
}

fn percentile_index(len: usize, percentile: usize) -> usize {
    len.saturating_mul(percentile)
        .div_ceil(100)
        .saturating_sub(1)
        .min(len - 1)
}

fn run_sample(old_text: &str, replacement: &str, mode: Mode) -> Sample {
    let started = Instant::now();
    let old_root = CropSnapshotLease::from_text(old_text);
    let old_root_build = started.elapsed();
    black_box(old_root.identity());

    let started = Instant::now();
    let (next_root, provenance) = old_root
        .edit(0..old_text.len(), replacement)
        .expect("whole-document edit is scalar aligned");
    let new_root_build = started.elapsed();
    black_box(&provenance);
    black_box(next_root.identity());

    let retained = matches!(mode, Mode::Retained).then(|| Arc::clone(&old_root));
    let mut current = old_root;
    black_box(current.identity());
    let started = Instant::now();
    current = next_root;
    black_box(current.identity());
    let admission = started.elapsed();

    let deferred_old_drop = retained.map_or(Duration::ZERO, |retained| {
        let started = Instant::now();
        drop(retained);
        started.elapsed()
    });

    let started = Instant::now();
    drop(current);
    let new_root_drop = started.elapsed();
    Sample {
        old_root_build,
        new_root_build,
        admission,
        deferred_old_drop,
        new_root_drop,
    }
}

fn measure_interleaved(
    old_text: &str,
    replacement: &str,
    samples: usize,
) -> (Vec<Sample>, Vec<Sample>) {
    black_box(run_sample(old_text, replacement, Mode::Direct));
    black_box(run_sample(old_text, replacement, Mode::Retained));
    let mut direct = Vec::with_capacity(samples);
    let mut retained = Vec::with_capacity(samples);
    for index in 0..samples {
        if index.is_multiple_of(2) {
            direct.push(run_sample(old_text, replacement, Mode::Direct));
            retained.push(run_sample(old_text, replacement, Mode::Retained));
        } else {
            retained.push(run_sample(old_text, replacement, Mode::Retained));
            direct.push(run_sample(old_text, replacement, Mode::Direct));
        }
    }
    (direct, retained)
}

fn print_stats(mib: usize, samples: &[Sample], mode: Mode) {
    let old_root = DurationStats::from_samples(samples.iter().map(|sample| sample.old_root_build));
    let new_root = DurationStats::from_samples(samples.iter().map(|sample| sample.new_root_build));
    let admission = DurationStats::from_samples(samples.iter().map(|sample| sample.admission));
    let deferred =
        DurationStats::from_samples(samples.iter().map(|sample| sample.deferred_old_drop));
    let new_drop = DurationStats::from_samples(samples.iter().map(|sample| sample.new_root_drop));
    println!(
        "crop_root_drop_receipt mib={mib} mode={} samples={} \
         old_build_ns_p50={} old_build_ns_p95={} old_build_ns_max={} \
         new_build_ns_p50={} new_build_ns_p95={} new_build_ns_max={} \
         admission_ns_p50={} admission_ns_p95={} admission_ns_max={} \
         deferred_drop_ns_p50={} deferred_drop_ns_p95={} deferred_drop_ns_max={} \
         new_drop_ns_p50={} new_drop_ns_p95={} new_drop_ns_max={}",
        mode.label(),
        samples.len(),
        old_root.p50_ns,
        old_root.p95_ns,
        old_root.max_ns,
        new_root.p50_ns,
        new_root.p95_ns,
        new_root.max_ns,
        admission.p50_ns,
        admission.p95_ns,
        admission.max_ns,
        deferred.p50_ns,
        deferred.p95_ns,
        deferred.max_ns,
        new_drop.p50_ns,
        new_drop.p95_ns,
        new_drop.max_ns,
    );
}

fn apply_lineage_edits(store: &mut SourceStore, count: usize) {
    for _ in 0..count {
        let replacement = if store.revision().0.is_multiple_of(2) {
            "b"
        } else {
            "a"
        };
        store
            .apply_edit(store.revision(), 0..1, replacement)
            .expect("one-byte lineage edit");
    }
}

fn run_lineage_sample(capacity: usize) -> LineageSample {
    let mut store = SourceStore::new("a", capacity);
    let started = Instant::now();
    apply_lineage_edits(&mut store, capacity);
    let fill = started.elapsed();

    let started = Instant::now();
    let historical = store
        .map_range_from(SourceRevision(0), 0..1)
        .expect("oldest retained lineage revision");
    let snapshot = started.elapsed();
    let retention = historical.retention();
    assert_eq!(retention.records, capacity);
    assert!(retention.tree_nodes <= retention.maximum_tree_nodes);
    black_box(historical.metrics());

    let started = Instant::now();
    apply_lineage_edits(&mut store, capacity);
    let overwrite = started.elapsed();
    assert!(store.map_range_from(SourceRevision(0), 0..1).is_err());

    let started = Instant::now();
    drop(historical);
    let historical_drop = started.elapsed();
    black_box(store.root_id());
    LineageSample {
        fill,
        snapshot,
        overwrite,
        historical_drop,
        snapshot_nodes: retention.tree_nodes,
    }
}

fn measure_lineage(capacity: usize, samples: usize) -> Vec<LineageSample> {
    black_box(run_lineage_sample(capacity));
    (0..samples).map(|_| run_lineage_sample(capacity)).collect()
}

fn print_lineage_stats(capacity: usize, samples: &[LineageSample]) {
    let fill = DurationStats::from_samples(samples.iter().map(|sample| sample.fill));
    let snapshot = DurationStats::from_samples(samples.iter().map(|sample| sample.snapshot));
    let overwrite = DurationStats::from_samples(samples.iter().map(|sample| sample.overwrite));
    let historical_drop =
        DurationStats::from_samples(samples.iter().map(|sample| sample.historical_drop));
    let minimum_nodes = samples
        .iter()
        .map(|sample| sample.snapshot_nodes)
        .min()
        .expect("nonempty samples");
    let maximum_nodes = samples
        .iter()
        .map(|sample| sample.snapshot_nodes)
        .max()
        .expect("nonempty samples");
    println!(
        "lineage_snapshot_drop_receipt capacity={capacity} samples={} \
         snapshot_nodes_min={minimum_nodes} snapshot_nodes_max={maximum_nodes} \
         fill_ns_p50={} fill_ns_p95={} fill_ns_max={} \
         snapshot_ns_p50={} snapshot_ns_p95={} snapshot_ns_max={} \
         overwrite_ns_p50={} overwrite_ns_p95={} overwrite_ns_max={} \
         historical_drop_ns_p50={} historical_drop_ns_p95={} historical_drop_ns_max={}",
        samples.len(),
        fill.p50_ns,
        fill.p95_ns,
        fill.max_ns,
        snapshot.p50_ns,
        snapshot.p95_ns,
        snapshot.max_ns,
        overwrite.p50_ns,
        overwrite.p95_ns,
        overwrite.max_ns,
        historical_drop.p50_ns,
        historical_drop.p95_ns,
        historical_drop.max_ns,
    );
}

fn parse_positive(value: Option<String>, name: &str) -> usize {
    value
        .unwrap_or_else(|| {
            panic!(
                "usage: crop_root_drop_probe root <MiB> <samples> | lineage <capacity> <samples>"
            )
        })
        .parse::<usize>()
        .unwrap_or_else(|_| panic!("{name} must be a positive integer"))
        .max(1)
}

fn main() {
    let mut arguments = env::args().skip(1);
    let command = arguments.next().unwrap_or_default();
    match command.as_str() {
        "root" => {
            let mib = parse_positive(arguments.next(), "MiB");
            let samples = parse_positive(arguments.next(), "samples");
            assert!(arguments.next().is_none(), "unexpected extra argument");
            let bytes = mib
                .checked_mul(1024 * 1024)
                .expect("fixture byte length fits usize");
            let old_text = String::from_utf8(vec![b'a'; bytes]).expect("ASCII fixture");
            let replacement = String::from_utf8(vec![b'b'; bytes]).expect("ASCII fixture");
            let (direct, retained) = measure_interleaved(&old_text, &replacement, samples);
            print_stats(mib, &direct, Mode::Direct);
            print_stats(mib, &retained, Mode::Retained);
        }
        "lineage" => {
            let capacity = parse_positive(arguments.next(), "capacity");
            let samples = parse_positive(arguments.next(), "samples");
            assert!(arguments.next().is_none(), "unexpected extra argument");
            let measured = measure_lineage(capacity, samples);
            print_lineage_stats(capacity, &measured);
        }
        _ => panic!(
            "usage: crop_root_drop_probe root <MiB> <samples> | lineage <capacity> <samples>"
        ),
    }
}
