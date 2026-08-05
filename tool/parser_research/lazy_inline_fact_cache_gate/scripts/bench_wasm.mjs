import fs from "node:fs";
import { performance } from "node:perf_hooks";

const path = process.argv[2] ??
  new URL("../wasm_probe/target/wasm32-unknown-unknown/release/flark_lazy_inline_fact_cache_wasm_probe.wasm", import.meta.url);
const bytes = fs.readFileSync(path);
const { instance } = await WebAssembly.instantiate(bytes, {});
const probe = instance.exports.lazy_window_probe;
if (typeof probe !== "function") throw new Error("missing lazy_window_probe export");
const behavior = instance.exports.lazy_behavior_probe;
if (typeof behavior !== "function") throw new Error("missing lazy_behavior_probe export");
const memoryBefore = instance.exports.memory.buffer.byteLength;
const behaviorMask = behavior();
if (behaviorMask !== 31) throw new Error(`behavior probe failed: ${behaviorMask}`);
const memoryAfterBehavior = instance.exports.memory.buffer.byteLength;

const leaves = Number(process.argv[3] ?? 64);
for (let index = 0; index < 5; index += 1) probe(leaves);
const memoryAfterWarmup = instance.exports.memory.buffer.byteLength;
const samples = [];
let checksum = 0n;
for (let index = 0; index < 100; index += 1) {
  const started = performance.now();
  checksum += probe(leaves);
  samples.push((performance.now() - started) * 1_000_000);
}
samples.sort((left, right) => left - right);
const percentile = (value) => samples[Math.floor((samples.length - 1) * value / 100)];
console.log(
  `backend=raw-wasm visible_leaves=${leaves} p50_ns=${Math.round(percentile(50))} ` +
  `p99_ns=${Math.round(percentile(99))} memory_before_bytes=${memoryBefore} ` +
  `memory_after_behavior_bytes=${memoryAfterBehavior} ` +
  `memory_after_warmup_bytes=${memoryAfterWarmup} ` +
  `memory_after_samples_bytes=${instance.exports.memory.buffer.byteLength} ` +
  `behavior_mask=${behaviorMask} checksum=${checksum}`,
);
