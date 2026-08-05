// Runs a `dart compile wasm -o <base>.wasm` result under Node/V8.
import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

const base = process.argv[2];
if (!base) {
  throw new Error('usage: node lazy_bulk_wasm_runner.mjs <output-base>');
}
const { compile } = await import(pathToFileURL(`${base}.mjs`));
const app = await compile(await readFile(`${base}.wasm`));
const instance = await app.instantiate({});
instance.invokeMain();
