import { readFile } from 'node:fs/promises';

const wasmPath = new URL(
  './target/wasm32-unknown-unknown/release/flark_parser_research.wasm',
  import.meta.url,
);
const bytes = await readFile(wasmPath);
const { instance } = await WebAssembly.instantiate(bytes, {});
const api = instance.exports;
const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true });

function withInput(text, body) {
  const value = encoder.encode(text);
  const pointer = value.length === 0 ? 0 : api.flark_v3_input_alloc(value.length);
  if (value.length > 0) {
    new Uint8Array(api.memory.buffer, pointer, value.length).set(value);
  }
  try {
    return body(pointer, value.length);
  } finally {
    if (value.length > 0) api.flark_v3_input_free(pointer, value.length);
  }
}

function create(text) {
  const handle = withInput(text, (pointer, length) =>
    api.flark_v3_document_new(pointer, length),
  );
  if (handle === 0) throw new Error('document_new failed');
  return handle;
}

function apply(handle, start, end, replacement) {
  return withInput(replacement, (pointer, length) =>
    api.flark_v3_document_apply(
      handle,
      api.flark_v3_document_revision(handle),
      api.flark_v3_document_hash32(handle),
      start,
      end,
      pointer,
      length,
    ),
  );
}

function materialize(handle) {
  const length = api.flark_v3_document_len(handle);
  if (length === 0) return '';
  const pointer = api.flark_v3_input_alloc(length);
  try {
    const copied = api.flark_v3_document_copy_text(handle, pointer, length);
    if (copied !== length) throw new Error(`copy_text copied ${copied}/${length}`);
    return decoder.decode(
      new Uint8Array(api.memory.buffer, pointer, length).slice(),
    );
  } finally {
    api.flark_v3_input_free(pointer, length);
  }
}

const unicode = create('a😀 café β\n');
if (apply(unicode, 1, 1, 'é') !== 0) throw new Error('Unicode edit failed');
if (materialize(unicode) !== 'aé😀 café β\n') {
  throw new Error('Unicode materialization diverged');
}
const unicodeHash = api.flark_v3_document_hash32(unicode);
const stale = withInput('x', (pointer, length) =>
  api.flark_v3_document_apply(unicode, 0, unicodeHash, 1, 1, pointer, length),
);
if (stale !== 2) throw new Error(`expected stale-revision status 2, got ${stale}`);
api.flark_v3_document_free(unicode);

const fragmentSamples = [];
withInput(
  'Paragraph with **bold**, *emphasis*, [a link](https://example.com), and '
    + '`code`.\n\n- item one\n- item two\n\n| left | right |\n| --- | --- |\n| a | b |\n',
  (pointer, length) => {
    for (let iteration = 0; iteration < 10_000; iteration += 1) {
      const started = process.hrtime.bigint();
      const nodeCount = api.flark_v3_comrak_parse_fragment(pointer, length);
      const elapsed = process.hrtime.bigint() - started;
      if (nodeCount === 0) throw new Error('Comrak fragment parse failed');
      fragmentSamples.push(Number(elapsed) / 1_000);
    }
  },
);
fragmentSamples.sort((left, right) => left - right);

let oracle = '';
for (let index = 0; oracle.length < 1_000_000; index += 1) {
  oracle += `paragraph ${index} with markdown-like ordinary text\n`;
}
const handle = create(oracle);
const samples = [];
for (let iteration = 0; iteration < 5_000; iteration += 1) {
  const offset = 32 + ((iteration * 7919) % (oracle.length - 64));
  const replacement = String.fromCharCode(97 + (iteration % 26));
  const started = process.hrtime.bigint();
  const status = apply(handle, offset, offset + 1, replacement);
  const elapsed = process.hrtime.bigint() - started;
  if (status !== 0) throw new Error(`apply failed with status ${status}`);
  oracle = `${oracle.slice(0, offset)}${replacement}${oracle.slice(offset + 1)}`;
  samples.push(Number(elapsed) / 1_000);
}
if (materialize(handle) !== oracle) throw new Error('large oracle diverged');
if (api.flark_v3_document_revision(handle) !== 5_000) {
  throw new Error('revision diverged');
}
samples.sort((left, right) => left - right);
const percentile = (value) =>
  samples[Math.floor(((samples.length - 1) * value) / 100)];
const fragmentPercentile = (value) =>
  fragmentSamples[Math.floor(((fragmentSamples.length - 1) * value) / 100)];
console.log(
  `flark_revisioned_wasm bytes=${api.flark_v3_document_len(handle)} ` +
    `cases=${samples.length} p50_us=${percentile(50).toFixed(1)} ` +
    `p95_us=${percentile(95).toFixed(1)} max_us=${samples.at(-1).toFixed(1)} ` +
    `comrak_fragment_cases=${fragmentSamples.length} ` +
    `comrak_fragment_p50_us=${fragmentPercentile(50).toFixed(1)} ` +
    `comrak_fragment_p95_us=${fragmentPercentile(95).toFixed(1)} ` +
    `comrak_fragment_max_us=${fragmentSamples.at(-1).toFixed(1)} ` +
    `revision=${api.flark_v3_document_revision(handle)}`,
);
api.flark_v3_document_free(handle);
