// Disposable V8 worker receipt. Node worker_threads are not a browser Web
// Worker proof, but unlike same-realm dart2js timings this exercises structured
// clone across an actual V8 worker boundary.
import {
  Worker,
  isMainThread,
  parentPort,
} from 'node:worker_threads';
import { performance } from 'node:perf_hooks';

if (!isMainThread) {
  parentPort.on('message', ({ id, source }) => {
    parentPort.postMessage({
      id,
      length: source.length,
      checksum: source.charCodeAt(0) ^ source.charCodeAt(source.length - 1),
    });
  });
} else {
  const worker = new Worker(new URL(import.meta.url));
  let nextId = 1;
  const pending = new Map();
  worker.on('message', (message) => {
    pending.get(message.id)(message);
    pending.delete(message.id);
  });

  const roundTrip = (source) => new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    const start = performance.now();
    worker.postMessage({ id, source });
    const sendMs = performance.now() - start;
    pending.set(id, (message) => resolve({ message, sendMs }));
  });

  await roundTrip('warmup');
  for (const sizeMiB of [1, 10, 100]) {
    const source = sourceOfLength(sizeMiB * 1024 * 1024);
    const iterations = sizeMiB === 100 ? 5 : (sizeMiB === 10 ? 20 : 50);
    const send = [];
    const total = [];
    let checksum = 0;
    for (let index = 0; index < iterations; index += 1) {
      const start = performance.now();
      const receipt = await roundTrip(source);
      total.push(performance.now() - start);
      send.push(receipt.sendMs);
      checksum ^= receipt.message.length ^ receipt.message.checksum;
    }
    console.log(JSON.stringify({
      receipt: 'node_v8_worker_string_clone',
      caveat: 'Node worker_threads, not browser Web Worker or Flutter web',
      size_mib: sizeMiB,
      iterations,
      send_call_ms: summary(send),
      roundtrip_ms: summary(total),
      checksum,
    }));
  }
  await worker.terminate();
}

function sourceOfLength(length) {
  const chunk =
    'Paragraph with **bold**, *emphasis*, `code`, [link][target], and text.\n';
  return chunk.repeat(Math.floor(length / chunk.length)) +
    chunk.slice(0, length % chunk.length);
}

function summary(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return {
    p50: sorted[Math.floor((sorted.length - 1) * 0.50)],
    p99: sorted[Math.floor((sorted.length - 1) * 0.99)],
    max: sorted.at(-1),
  };
}
