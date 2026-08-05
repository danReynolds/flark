import fs from "node:fs";

const [wasmPath, sizeArg = "8192", shapeArg = "0", iterationsArg = "2000"] = process.argv.slice(2);
if (!wasmPath) throw new Error("usage: node bench_wasm.mjs module.wasm bytes shape iterations");
const size = Number(sizeArg);
const shape = Number(shapeArg);
const iterations = Number(iterationsArg);
const wasm = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});
const { inline_fragment_prepare: prepare, inline_fragment_sample: sample } = wasm.instance.exports;
prepare(size, shape);
for (let i = 0; i < 100; i++) sample();
const samples = [];
let receipt = 0n;
for (let i = 0; i < iterations; i++) {
  const started = process.hrtime.bigint();
  receipt = sample();
  samples.push(Number(process.hrtime.bigint() - started));
}
samples.sort((a, b) => a - b);
const percentile = (p) => samples[Math.floor((samples.length - 1) * p / 100)];
const rejected = receipt === -1n;
const facts = rejected ? 0 : Number(receipt >> 32n);
const output = rejected ? 0 : Number(receipt & 0xffffffffn);
const wasmMemory = wasm.instance.exports.memory.buffer.byteLength;
console.log(`backend=wasm shape=${shape} bytes=${size} iterations=${iterations} p50_ns=${percentile(50)} p99_ns=${percentile(99)} max_ns=${samples.at(-1)} facts=${facts} output_bytes=${output} rejected=${rejected} wasm_memory_bytes=${wasmMemory} rss_bytes=${process.memoryUsage().rss}`);
