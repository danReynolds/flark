import { readFile } from 'node:fs/promises';

const wasmPath = new URL(
  './target/wasm32-unknown-unknown/release/flark_parser_research.wasm',
  import.meta.url,
);
const bytes = await readFile(wasmPath);
const { instance } = await WebAssembly.instantiate(bytes, {});
const api = instance.exports;
const encoder = new TextEncoder();

function activeShardOfSize(targetLength) {
  let output = '';
  for (let index = 0; output.length < targetLength; index += 1) {
    output += `word${index} **bold** *em* \`code\` `;
  }
  return output.slice(0, targetLength);
}

function plainShardOfSize(targetLength) {
  const unit = 'ordinary words without markdown delimiters ';
  return unit.repeat(Math.ceil(targetLength / unit.length)).slice(0, targetLength);
}

function measure(workload, markdown, warmups, measured) {
  const input = encoder.encode(markdown);
  const pointer = api.flark_v3_input_alloc(input.length);
  new Uint8Array(api.memory.buffer, pointer, input.length).set(input);
  const samples = [];
  let nodes = 0;
  try {
    for (let iteration = 0; iteration < warmups + measured; iteration += 1) {
      const started = process.hrtime.bigint();
      nodes = api.flark_v3_comrak_parse_fragment(pointer, input.length);
      const micros = Number(process.hrtime.bigint() - started) / 1_000;
      if (nodes === 0) throw new Error('Comrak parse failed');
      if (iteration >= warmups) samples.push(micros);
    }
  } finally {
    api.flark_v3_input_free(pointer, input.length);
  }
  samples.sort((left, right) => left - right);
  const percentile = (value) =>
    samples[Math.floor(((samples.length - 1) * value) / 100)];
  console.log(
    `flark_comrak_fragment_wasm workload=${workload} bytes=${markdown.length} ` +
      `nodes=${nodes} p50_us=${percentile(50).toFixed(1)} ` +
      `p95_us=${percentile(95).toFixed(1)} max_us=${samples.at(-1).toFixed(1)}`,
  );
}

for (const config of [
  { bytes: 64, warmups: 8, measured: 40 },
  { bytes: 1_024, warmups: 8, measured: 40 },
  { bytes: 4_096, warmups: 6, measured: 30 },
  { bytes: 16_384, warmups: 4, measured: 15 },
  { bytes: 65_536, warmups: 2, measured: 8 },
]) {
  measure(
    'token_dense',
    activeShardOfSize(config.bytes),
    config.warmups,
    config.measured,
  );
}
for (const config of [
  { bytes: 4_096, warmups: 6, measured: 30 },
  { bytes: 65_536, warmups: 2, measured: 8 },
]) {
  measure(
    'plain',
    plainShardOfSize(config.bytes),
    config.warmups,
    config.measured,
  );
}
