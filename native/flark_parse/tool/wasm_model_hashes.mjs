// Runs the corpora through the wasm build and prints FNV-1a 64 hashes of the
// render model, in the same format as `model_hashes`, for byte-identity checks.
import { readFileSync } from 'node:fs';
const [,, wasmPath, corpusDir] = process.argv;
const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {});
const ex = instance.exports;
const enc = new TextEncoder();
function fnv1a(bytes) { let h = 0xcbf29ce484222325n; for (const b of bytes) { h ^= BigInt(b); h = (h * 0x100000001b3n) & 0xffffffffffffffffn; } return h.toString(16).padStart(16, '0'); }
const outCell = ex.flark_parse_alloc(16);
for (const file of ['common_mark_tests.json', 'gfm_tests.json']) {
  const cases = JSON.parse(readFileSync(`${corpusDir}/${file}`, 'utf8'));
  for (const c of cases) {
    const bytes = enc.encode(c.markdown);
    const input = ex.flark_parse_alloc(bytes.length + 1);
    new Uint8Array(ex.memory.buffer).set(bytes, input);
    const rc = ex.flark_parse(input, bytes.length, outCell, outCell + 8);
    if (rc !== 0) { console.log(`${file}#${c.example} ERROR ${rc}`); continue; }
    const view = new DataView(ex.memory.buffer);
    const ptr = view.getUint32(outCell, true), len = view.getUint32(outCell + 8, true);
    const model = new Uint8Array(ex.memory.buffer, ptr, len);
    console.log(`${file}#${c.example} ${len} ${fnv1a(model)}`);
    ex.flark_parse_free(ptr, len);
    ex.flark_parse_free(input, bytes.length + 1);
  }
}
