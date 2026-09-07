// Transport only: the same released Rust/Wasm engine owns all syntax behavior.
// One call at a time is enforced by the Dart queue. Outputs transfer ownership.
let engine;
const encoder = new TextEncoder();
self.onmessage = async ({data}) => {
  try {
    if (data.kind === 'init') {
      const response = await fetch(data.wasm);
      if (!response.ok) throw new Error(`Wasm HTTP ${response.status}`);
      const {instance} = await WebAssembly.instantiate(await response.arrayBuffer(), {});
      engine = instance.exports;
      if (engine.flark_tree_sitter_version() !== 4) throw new Error('Expected worker ABI 4');
      self.postMessage({kind: 'ready'});
      return;
    }
    if (!engine || data.kind !== 'analyze') throw new Error('Worker is not initialized');
    const source = encoder.encode(data.source);
    const input = engine.flark_tree_sitter_alloc(source.length);
    const cell = engine.flark_tree_sitter_alloc(16);
    let output = 0, length = 0;
    let bytes;
    // A trap takes the fatal path below; no calls are made into that instance
    // afterward. Normal errors still release all live allocations.
    new Uint8Array(engine.memory.buffer).set(source, input);
    const status = engine.flark_tree_sitter_analyze(input, source.length, data.language, cell, cell + 8);
    const view = new DataView(engine.memory.buffer);
    output = view.getUint32(cell, true);
    length = view.getUint32(cell + 8, true);
    try {
      if (status !== 0) throw new Error(`Analysis failed (${status})`);
      bytes = new Uint8Array(engine.memory.buffer, output, length).slice();
    } finally {
      engine.flark_tree_sitter_free(output, length);
      engine.flark_tree_sitter_free(input, source.length);
      engine.flark_tree_sitter_free(cell, 16);
    }
    self.postMessage({kind: 'result', bytes}, [bytes.buffer]);
  } catch (error) {
    engine = undefined;
    self.postMessage({kind: 'error', error: String(error)});
    self.close();
  }
};
