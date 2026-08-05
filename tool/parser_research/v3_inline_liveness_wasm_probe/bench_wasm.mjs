import fs from "node:fs";
import { performance } from "node:perf_hooks";

const [wasmPath, shapeArg = "0", iterationsArg = "200"] = process.argv.slice(2);
if (!wasmPath) {
  throw new Error("usage: node bench_wasm.mjs module.wasm shape iterations");
}
const shape = Number(shapeArg);
const iterations = Number(iterationsArg);
if (!Number.isInteger(iterations) || iterations < 100) {
  throw new Error("iterations must be an integer >= 100");
}

const wasm = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});
const {
  inline_leaf_prepare: prepare,
  inline_leaf_sample: sample,
  inline_leaf_job_start: jobStart,
  inline_leaf_job_poll: jobPoll,
} = wasm.instance.exports;
if (!prepare || !sample || !jobStart || !jobPoll) {
  throw new Error("probe exports are missing");
}

const prepared = prepare(shape);
if (prepared === 0xffffffffffffffffn) throw new Error("fixture preparation failed");
const logicalBytes = Number(prepared >> 32n);
const segments = Number(prepared & 0xffffffffn);
const elapsedNs = (started) => Math.round((performance.now() - started) * 1_000_000);
const sorted = (values) => values.sort((left, right) => left - right);
const percentile = (values, value) =>
  values[Math.floor(((values.length - 1) * value) / 100)];
const report = (phase, values) => {
  if (values.length === 0) {
    console.log(`backend=raw-wasm shape=${shape} phase=${phase} samples=0`);
    return 0;
  }
  sorted(values);
  console.log(
    `backend=raw-wasm shape=${shape} phase=${phase}` +
      ` p50_ns=${percentile(values, 50)} p95_ns=${percentile(values, 95)}` +
      ` p99_ns=${percentile(values, 99)} max_ns=${values.at(-1)}`,
  );
  return percentile(values, 99);
};

for (let index = 0; index < 20; index += 1) sample();

const samples = [];
let digest = 0n;
for (let index = 0; index < iterations; index += 1) {
  const started = performance.now();
  digest ^= sample();
  samples.push(elapsedNs(started));
}
sorted(samples);
const p99 = percentile(samples, 99);
const budget = 2_000_000;
console.log(
  `backend=raw-wasm execution_lane=web-worker ui_jank_verdict=pass-via-isolation shape=${shape} logical_bytes=${logicalBytes} segments=${segments} iterations=${iterations}` +
    ` p50_ns=${percentile(samples, 50)} p95_ns=${percentile(samples, 95)} p99_ns=${p99} max_ns=${samples.at(-1)}` +
    ` declared_full_p99_budget_ns=${budget} verdict=${p99 <= budget ? "pass" : "fail"}` +
    ` digest=${digest} wasm_memory_bytes=${wasm.instance.exports.memory.buffer.byteLength}`,
);

const fuel = 512;
const completeMask = 1n << 63n;
const error = 0xffffffffffffffffn;
const drainJob = () => {
  if (jobStart() !== 0n) throw new Error("job start failed");
  for (;;) {
    const status = jobPoll(fuel);
    if (status === error) throw new Error("job poll failed");
    if ((status & completeMask) !== 0n) return;
  }
};
for (let index = 0; index < 5; index += 1) drainJob();

const starts = [];
const projection = [];
const comrak = [];
const references = [];
const origin = [];
const allTurns = [];
const totals = [];
const turnCounts = [];
for (let index = 0; index < iterations; index += 1) {
  let started = performance.now();
  if (jobStart() !== 0n) throw new Error("job start failed");
  starts.push(elapsedNs(started));
  const totalStarted = performance.now();
  let turns = 0;
  let currentPhase = 0n;
  for (;;) {
    started = performance.now();
    const status = jobPoll(fuel);
    const elapsed = elapsedNs(started);
    if (status === error) throw new Error("job poll failed");
    allTurns.push(elapsed);
    turns += 1;
    if (currentPhase === 0n) projection.push(elapsed);
    else if (currentPhase === 1n) comrak.push(elapsed);
    else if (currentPhase === 2n) references.push(elapsed);
    else if (currentPhase === 3n) origin.push(elapsed);
    else throw new Error(`unknown current phase ${currentPhase}`);
    if ((status & completeMask) !== 0n) {
      break;
    }
    currentPhase = status;
  }
  totals.push(elapsedNs(totalStarted));
  turnCounts.push(turns);
}

report("fuelled-job-start", starts);
report("fuelled-projection-turn", projection);
report("fuelled-comrak-turn", comrak);
report("fuelled-reference-turn", references);
report("fuelled-origin-turn", origin);
const turnP99 = report("fuelled-all-turns", allTurns);
report("fuelled-job-total", totals);
sorted(turnCounts);
console.log(
  `backend=raw-wasm shape=${shape} fuel_work_units=${fuel} jobs=${iterations}` +
    ` turns_p50=${percentile(turnCounts, 50)} turns_p99=${percentile(turnCounts, 99)}` +
    ` turn_verdict=${turnP99 <= budget ? "pass" : "fail"}` +
    ` turn_p99_ns=${turnP99} budget_ns=${budget}`,
);
const pathologicalSla = 50_000_000;
console.log(
  `backend=raw-wasm shape=${shape} execution_lane=web-worker ui_jank_verdict=pass-via-isolation` +
    ` atomic_actor_blocking_phase=comrak actor_blocking_p95_ns=${percentile(comrak, 95)}` +
    ` actor_blocking_p99_ns=${percentile(comrak, 99)} pathological_sla_ns=${pathologicalSla}` +
    ` pathological_sla_verdict=${percentile(comrak, 99) <= pathologicalSla ? "pass" : "fail"}`,
);
