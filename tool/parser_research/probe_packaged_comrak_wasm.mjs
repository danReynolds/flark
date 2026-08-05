import { readFile } from 'node:fs/promises';

const wasmPath = new URL(
  '../../lib/assets/wasm/flark_comrak_bridge.wasm',
  import.meta.url,
);
const bytes = await readFile(wasmPath);
const { instance } = await WebAssembly.instantiate(bytes, {});
const api = instance.exports;
const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true });

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

function parse(markdown, revision) {
  const input = encoder.encode(markdown);
  const inputPointer = api.flark_comrak_input_alloc(input.length);
  new Uint8Array(api.memory.buffer, inputPointer, input.length).set(input);
  try {
    const parseStarted = process.hrtime.bigint();
    const responsePointer = api.flark_comrak_parse(
      revision,
      1,
      inputPointer,
      input.length,
    );
    const parseMicros = Number(process.hrtime.bigint() - parseStarted) / 1_000;
    if (responsePointer === 0) throw new Error('null parse response');
    try {
      const view = new DataView(api.memory.buffer);
      const status = view.getUint16(responsePointer + 8, true);
      const payloadPointer = view.getUint32(responsePointer + 12, true);
      const payloadLength = view.getUint32(responsePointer + 16, true);
      if (status !== 0) throw new Error(`parse status ${status}`);

      const copyStarted = process.hrtime.bigint();
      const payload = new Uint8Array(
        api.memory.buffer,
        payloadPointer,
        payloadLength,
      ).slice();
      const copyMicros = Number(process.hrtime.bigint() - copyStarted) / 1_000;

      const decodeStarted = process.hrtime.bigint();
      const decoded = JSON.parse(decoder.decode(payload));
      const decodeMicros = Number(process.hrtime.bigint() - decodeStarted) / 1_000;
      return {
        parseMicros,
        copyMicros,
        decodeMicros,
        payloadLength,
        inlineTokens: decoded.inlineTokens?.length ?? 0,
      };
    } finally {
      api.flark_comrak_response_free(responsePointer);
    }
  } finally {
    api.flark_comrak_input_free(inputPointer, input.length);
  }
}

function percentile(samples, value) {
  return samples[Math.floor(((samples.length - 1) * value) / 100)];
}

function measure(workload, markdown, warmups, measured) {
  const parseSamples = [];
  const copySamples = [];
  const decodeSamples = [];
  let payloadLength = 0;
  let inlineTokens = 0;
  for (let iteration = 0; iteration < warmups + measured; iteration += 1) {
    const result = parse(markdown, iteration + 1);
    if (iteration < warmups) continue;
    parseSamples.push(result.parseMicros);
    copySamples.push(result.copyMicros);
    decodeSamples.push(result.decodeMicros);
    payloadLength = result.payloadLength;
    inlineTokens = result.inlineTokens;
  }
  for (const samples of [parseSamples, copySamples, decodeSamples]) {
    samples.sort((left, right) => left - right);
  }
  console.log(
    `flark_packaged_wasm workload=${workload} bytes=${markdown.length} ` +
      `inline_tokens=${inlineTokens} payload_bytes=${payloadLength} ` +
      `parse_p50_us=${percentile(parseSamples, 50).toFixed(1)} ` +
      `parse_p95_us=${percentile(parseSamples, 95).toFixed(1)} ` +
      `copy_p95_us=${percentile(copySamples, 95).toFixed(1)} ` +
      `json_decode_p95_us=${percentile(decodeSamples, 95).toFixed(1)}`,
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
