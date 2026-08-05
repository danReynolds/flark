use std::hint::black_box;
use std::time::Instant;

use flark_v3_runtime_slice::{
    InlineLeafMaterializationFuel, InlineLeafMaterializationPhase,
    InlineLeafMaterializationProgress, InlineLivenessFixture, InlineLivenessShape,
};

const NATIVE_P99_TURN_BUDGET_NS: u64 = 1_000_000;
const PATHOLOGICAL_ACTOR_BLOCKING_SLA_NS: u64 = 50_000_000;
const MATERIALIZATION_FUEL_PER_TURN: usize = 512;

#[derive(Clone, Copy, Debug)]
struct Distribution {
    p50: u64,
    p95: u64,
    p99: u64,
    max: u64,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let requested = args.next().unwrap_or_else(|| "all".to_owned());
    let iterations = args
        .next()
        .unwrap_or_else(|| "200".to_owned())
        .parse::<usize>()
        .expect("iterations must be an integer");
    assert!(
        iterations >= 100,
        "percentile runs require at least 100 samples"
    );

    let shapes: &[InlineLivenessShape] = match requested.as_str() {
        "fragmented" | "fragmented-8k" => &[InlineLivenessShape::Fragmented8KiB],
        "hidden" | "hidden-cap" => &[InlineLivenessShape::HiddenAtSegmentCap],
        "references" | "reference-dense" | "reference-dense-8k" => {
            &[InlineLivenessShape::ReferenceDense8KiB]
        }
        "enclosing" | "fragmented-enclosing" | "fragmented-enclosing-emphasis" => {
            &[InlineLivenessShape::FragmentedEnclosingEmphasis8KiB]
        }
        "all" => &[
            InlineLivenessShape::Fragmented8KiB,
            InlineLivenessShape::HiddenAtSegmentCap,
            InlineLivenessShape::ReferenceDense8KiB,
            InlineLivenessShape::FragmentedEnclosingEmphasis8KiB,
        ],
        other => {
            panic!("unknown shape {other:?}; use fragmented, hidden, references, enclosing, or all")
        }
    };

    for shape in shapes {
        run_shape(*shape, iterations);
    }
}

fn run_shape(shape: InlineLivenessShape, iterations: usize) {
    let fixture = InlineLivenessFixture::new(shape);
    let metadata = fixture.metadata();
    let fragment = fixture.parse_fragment();
    let composition = fixture.composition_metadata(&fragment);
    println!(
        "backend=native execution_lane=parser-isolate-or-worker ui_jank_verdict=pass-via-isolation shape={} source_bytes={} logical_bytes={} logical_utf16={} segments={} program_runs={} copy_ranges={} copied_bytes={} origin_runs={} reference_symbols={} reference_dependencies={} semantic_facts={} projection_facts={} mapped_physical_parts={} iterations={} declared_full_p99_budget_ns={}",
        shape.label(),
        metadata.source_bytes,
        metadata.logical_bytes,
        metadata.logical_utf16,
        metadata.logical_segments,
        metadata.projection_program_runs,
        metadata.source_copy_ranges,
        metadata.source_bytes_copied,
        metadata.origin_runs,
        metadata.reference_symbols,
        composition.reference_dependencies,
        composition.semantic_facts,
        composition.projection_facts,
        composition.mapped_physical_parts,
        iterations,
        NATIVE_P99_TURN_BUDGET_NS,
    );

    report(
        shape,
        "green",
        measure(iterations, || fixture.sample_green_traversal()),
    );
    report(
        shape,
        "source-copy",
        measure(iterations, || fixture.sample_source_copy()),
    );
    report(
        shape,
        "comrak",
        measure(iterations, || fixture.sample_comrak()),
    );
    report(
        shape,
        "origin-compose",
        measure(iterations, || fixture.sample_origin_composition(&fragment)),
    );
    let full = measure(iterations, || fixture.sample_full_materializer());
    report(shape, "full", full);
    println!(
        "backend=native shape={} verdict={} full_p99_ns={} budget_ns={}",
        shape.label(),
        if full.p99 <= NATIVE_P99_TURN_BUDGET_NS {
            "pass"
        } else {
            "fail"
        },
        full.p99,
        NATIVE_P99_TURN_BUDGET_NS,
    );
    measure_fuelled_turns(&fixture, iterations, MATERIALIZATION_FUEL_PER_TURN);
}

fn measure_fuelled_turns(fixture: &InlineLivenessFixture, jobs: usize, work_units: usize) {
    let fuel = InlineLeafMaterializationFuel::new(work_units).expect("nonzero benchmark fuel");
    for _ in 0..5 {
        drain_job(fixture, fuel);
    }

    let mut starts = Vec::with_capacity(jobs);
    let mut projection = Vec::new();
    let mut comrak = Vec::with_capacity(jobs);
    let mut references = Vec::new();
    let mut origin = Vec::with_capacity(jobs);
    let mut all_turns = Vec::new();
    let mut totals = Vec::with_capacity(jobs);
    let mut turn_counts = Vec::with_capacity(jobs);
    for _ in 0..jobs {
        let started = Instant::now();
        let mut job = black_box(fixture.start_job());
        starts.push(elapsed_ns(started));
        let job_started = Instant::now();
        let mut turns = 0_u64;
        let mut current_phase = InlineLeafMaterializationPhase::Projection;
        loop {
            let started = Instant::now();
            let progress = black_box(fixture.poll_job(&mut job, fuel));
            let elapsed = elapsed_ns(started);
            all_turns.push(elapsed);
            turns += 1;
            match current_phase {
                InlineLeafMaterializationPhase::Projection => projection.push(elapsed),
                InlineLeafMaterializationPhase::InlineService => comrak.push(elapsed),
                InlineLeafMaterializationPhase::ReferenceValidation => references.push(elapsed),
                InlineLeafMaterializationPhase::OriginComposition => origin.push(elapsed),
            }
            match progress {
                InlineLeafMaterializationProgress::Pending { phase, .. } => current_phase = phase,
                InlineLeafMaterializationProgress::Complete(_) => break,
            }
        }
        totals.push(elapsed_ns(job_started));
        turn_counts.push(turns);
    }

    report(fixture.shape(), "fuelled-job-start", distribution(starts));
    report(
        fixture.shape(),
        "fuelled-projection-turn",
        distribution(projection),
    );
    let comrak = distribution(comrak);
    report(fixture.shape(), "fuelled-comrak-turn", comrak);
    report(
        fixture.shape(),
        "fuelled-reference-turn",
        distribution(references),
    );
    report(fixture.shape(), "fuelled-origin-turn", distribution(origin));
    let all_turns = distribution(all_turns);
    report(fixture.shape(), "fuelled-all-turns", all_turns);
    report(fixture.shape(), "fuelled-job-total", distribution(totals));
    turn_counts.sort_unstable();
    println!(
        "backend=native shape={} fuel_work_units={} jobs={} turns_p50={} turns_p99={} turn_verdict={} turn_p99_ns={} budget_ns={}",
        fixture.shape().label(),
        work_units,
        jobs,
        percentile(&turn_counts, 50),
        percentile(&turn_counts, 99),
        if all_turns.p99 <= NATIVE_P99_TURN_BUDGET_NS {
            "pass"
        } else {
            "fail"
        },
        all_turns.p99,
        NATIVE_P99_TURN_BUDGET_NS,
    );
    println!(
        "backend=native shape={} execution_lane=parser-isolate-or-worker ui_jank_verdict=pass-via-isolation atomic_actor_blocking_phase=comrak actor_blocking_p95_ns={} actor_blocking_p99_ns={} pathological_sla_ns={} pathological_sla_verdict={}",
        fixture.shape().label(),
        comrak.p95,
        comrak.p99,
        PATHOLOGICAL_ACTOR_BLOCKING_SLA_NS,
        if comrak.p99 <= PATHOLOGICAL_ACTOR_BLOCKING_SLA_NS {
            "pass"
        } else {
            "fail"
        },
    );
}

fn drain_job(fixture: &InlineLivenessFixture, fuel: InlineLeafMaterializationFuel) {
    let mut job = fixture.start_job();
    while !matches!(
        fixture.poll_job(&mut job, fuel),
        InlineLeafMaterializationProgress::Complete(_)
    ) {}
}

fn measure(mut iterations: usize, mut sample: impl FnMut() -> u64) -> Distribution {
    let warmups = iterations.clamp(20, 100) / 5;
    for _ in 0..warmups {
        black_box(sample());
    }
    let mut samples = Vec::with_capacity(iterations);
    while iterations != 0 {
        let started = Instant::now();
        black_box(sample());
        samples.push(elapsed_ns(started));
        iterations -= 1;
    }
    distribution(samples)
}

fn distribution(mut samples: Vec<u64>) -> Distribution {
    samples.sort_unstable();
    Distribution {
        p50: percentile(&samples, 50),
        p95: percentile(&samples, 95),
        p99: percentile(&samples, 99),
        max: samples.last().copied().unwrap_or(0),
    }
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    samples[(samples.len().saturating_sub(1) * percentile) / 100]
}

fn report(shape: InlineLivenessShape, phase: &str, distribution: Distribution) {
    println!(
        "backend=native shape={} phase={} p50_ns={} p95_ns={} p99_ns={} max_ns={}",
        shape.label(),
        phase,
        distribution.p50,
        distribution.p95,
        distribution.p99,
        distribution.max,
    );
}
