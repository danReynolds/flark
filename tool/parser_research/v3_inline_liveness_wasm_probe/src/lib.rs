use std::cell::RefCell;

use flark_v3_runtime_slice::{
    InlineLeafMaterializationFuel, InlineLeafMaterializationJob, InlineLeafMaterializationPhase,
    InlineLeafMaterializationProgress, InlineLeafOutcome, InlineLivenessFixture,
    InlineLivenessShape,
};

const PROBE_ERROR: u64 = u64::MAX;
const JOB_COMPLETE: u64 = 1 << 63;
const JOB_UNKNOWN: u64 = 1 << 62;
const PHASE_PROJECTION: u64 = 0;
const PHASE_INLINE_SERVICE: u64 = 1;
const PHASE_REFERENCE_VALIDATION: u64 = 2;
const PHASE_ORIGIN_COMPOSITION: u64 = 3;

struct ProbeState {
    fixture: InlineLivenessFixture,
    job: Option<InlineLeafMaterializationJob>,
}

thread_local! {
    static STATE: RefCell<Option<ProbeState>> = const { RefCell::new(None) };
}

#[unsafe(no_mangle)]
pub extern "C" fn inline_leaf_prepare(shape: u32) -> u64 {
    let Some(shape) = InlineLivenessShape::from_probe_code(shape) else {
        return PROBE_ERROR;
    };
    let fixture = InlineLivenessFixture::new(shape);
    let metadata = fixture.metadata();
    let result = (u64::try_from(metadata.logical_bytes).unwrap_or(u64::MAX) << 32)
        | u64::try_from(metadata.logical_segments).unwrap_or(u64::MAX);
    STATE.with(|slot| {
        *slot.borrow_mut() = Some(ProbeState { fixture, job: None });
    });
    result
}

#[unsafe(no_mangle)]
pub extern "C" fn inline_leaf_sample() -> u64 {
    STATE.with(|slot| {
        slot.borrow().as_ref().map_or(PROBE_ERROR, |state| {
            state.fixture.sample_full_materializer()
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn inline_leaf_job_start() -> u64 {
    STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let Some(state) = state.as_mut() else {
            return PROBE_ERROR;
        };
        state.job = Some(state.fixture.start_job());
        0
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn inline_leaf_job_poll(work_units: u32) -> u64 {
    let Some(fuel) = InlineLeafMaterializationFuel::new(work_units as usize) else {
        return PROBE_ERROR;
    };
    STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let Some(state) = state.as_mut() else {
            return PROBE_ERROR;
        };
        let Some(mut job) = state.job.take() else {
            return PROBE_ERROR;
        };
        match state.fixture.poll_job(&mut job, fuel) {
            InlineLeafMaterializationProgress::Pending { phase, .. } => {
                state.job = Some(job);
                match phase {
                    InlineLeafMaterializationPhase::Projection => PHASE_PROJECTION,
                    InlineLeafMaterializationPhase::InlineService => PHASE_INLINE_SERVICE,
                    InlineLeafMaterializationPhase::ReferenceValidation => {
                        PHASE_REFERENCE_VALIDATION
                    }
                    InlineLeafMaterializationPhase::OriginComposition => PHASE_ORIGIN_COMPOSITION,
                }
            }
            InlineLeafMaterializationProgress::Complete(InlineLeafOutcome::Ready(_)) => {
                JOB_COMPLETE
            }
            InlineLeafMaterializationProgress::Complete(InlineLeafOutcome::Unknown(_)) => {
                JOB_COMPLETE | JOB_UNKNOWN
            }
        }
    })
}
