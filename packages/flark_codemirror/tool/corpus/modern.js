#!/usr/bin/env node
'use strict';
import fs, { readFile as read, writeFile } from 'node:fs/promises';
import * as path from "node:path";
export { read, writeFile as write };
export default class Store extends EventTarget {
  #items = new Map();
  static count = 0;
  constructor(name = 'store', { limit = 10, ...rest } = {}) {
    super();
    this.name = name;
    this.limit = limit ?? rest.limit;
  }
  get size() { return this.#items.size; }
  set size(value) { throw new TypeError(`size is ${value}`); }
  async *entries() {
    for await (const [key, value] of this.#items) yield [key, value];
  }
  static #secret() { return new.target; }
}

const pattern = /[a-z]+\/(?<id>\d+)/giu, half = 1 / 2, big = 1_000_000n;
const hex = 0xFF_FF, oct = 0o755, bin = 0b1010, exp = 1.5e-3, dot = .5;
let total = items.reduce((sum, { price, qty = 1 }) => sum + price * qty, 0);
const nested = `outer ${inner ? `inner ${deep}` : 'plain'} tail`;
const fn = async (a, b = {}, ...others) => {
  await Promise.all(others.map(o => o?.value ?? a));
  return b?.[a]?.();
};

label: for (let i = 0; i < 10; i++) {
  if (i % 2) continue label;
  else if (i > 8) break label;
  switch (i) {
    case 1:
    case 2:
      console.log(i);
      break;
    default:
      void 0;
  }
}

try {
  risky();
} catch ({ message }) {
  console.error(message);
} finally {
  cleanup();
}

do {
  x **= 2;
  y >>>= 1;
  z ||= w &&= v ??= u;
} while (x < 100 && !done);

function* gen() {
  const r = yield* other();
  return typeof r === 'object' ? r : null;
}

const obj = {
  'quoted key': 1,
  42: 'answer',
  [computed]: true,
  method() {},
  get prop() { return this._p; },
  async fetch() { return await load(); },
  ...spread,
};
a = b
  ? c
  : d;
const re2 = x / y / z, re3 = (/ab+c/).test(s);
if (a) /regex/.exec(b);
x = y <!-- comment
--> also a comment
delete obj[key], typeof x, void 0, x instanceof Y, 'k' in obj;
debugger;
with (scope) { value; }
function makeFields() {
  return class Fields {
    /** @type {number} */
    #target
    #kind
    static #count = 0
    #index;
    #run() { return this.#kind in this; }
  };
}
