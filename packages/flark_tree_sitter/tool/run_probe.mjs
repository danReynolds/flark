import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

globalThis.self = globalThis;
globalThis.flarkTreeSitterBytes = new Uint8Array(readFileSync(new URL('../lib/assets/wasm/flark_tree_sitter.wasm', import.meta.url)));
const path = process.argv[2];
if (!path) throw new Error('Usage: node tool/run_probe.mjs <compiled JS or .mjs>');
if (path.endsWith('.mjs')) {
  const {compile} = await import(pathToFileURL(path).href);
  const bytes = readFileSync(path.replace(/\.mjs$/, '.wasm'));
  const module = await compile(bytes);
  const app = await module.instantiate({});
  app.invokeMain();
} else {
  await import(pathToFileURL(path).href);
}
