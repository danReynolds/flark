// Records CodeMirror 5's own tokens and indentation for the Dart port's
// reference test. Needs Node and an unpacked `codemirror@5.65.21`:
//
//   npm pack codemirror@5.65.21 && tar xzf codemirror-5.65.21.tgz
//   node tool/reference/generate.cjs package test/fixtures/reference.json
//
// A third argument names another corpus directory, for wider local runs
// (FLARK_CODEMIRROR_REFERENCE points the Dart test at their output).
//
// Cases are the files in tool/corpus, two of CodeMirror's own sources, and
// seeded mutations of short windows of them (deleted characters, stray
// brackets and quotes, tabs, CRLF), since an editor mostly sees incomplete
// code. For each line the fixture holds the indentation CodeMirror computes
// for the line as written and for an empty line in its place (what Enter
// asks), then each token as start, end and an index into `styles`.
'use strict';
const fs = require('fs');
const path = require('path');

const [cmDir, outPath, otherCorpus] = process.argv.slice(2);
if (!cmDir || !outPath) {
  console.error('usage: generate.cjs <codemirror package dir> <out.json>');
  process.exit(2);
}
const pkg = JSON.parse(fs.readFileSync(path.join(cmDir, 'package.json'), 'utf8'));
if (pkg.version !== '5.65.21') throw new Error(`codemirror ${pkg.version}, expected 5.65.21`);
const CM = require(path.resolve(cmDir, 'addon/runmode/runmode.node.js'));
require(path.resolve(cmDir, 'mode/javascript/javascript.js'));

const TAB_SIZE = 4, INDENT_UNIT = 2;
const modes = {
  '.js': 'javascript', '.cjs': 'javascript', '.ts': 'typescript',
  '.json': 'json', '.jsonld': 'jsonld',
};
const specs = {
  javascript: { name: 'javascript' },
  typescript: { name: 'javascript', typescript: true },
  json: { name: 'javascript', json: true },
  jsonld: { name: 'javascript', jsonld: true },
};

const styles = [null], styleIds = new Map([[null, 0]]);
function styleId(style) {
  const key = style === undefined ? null : style;
  if (!styleIds.has(key)) { styleIds.set(key, styles.length); styles.push(key); }
  return styleIds.get(key);
}

function run(text, modeName) {
  const mode = CM.getMode({ indentUnit: INDENT_UNIT, tabSize: TAB_SIZE }, specs[modeName]);
  const lines = CM.splitLines(text);
  const state = CM.startState(mode);
  const out = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const lead = /^\s*/.exec(line)[0].length;
    // CodeMirror.Pass is undefined in the Node shim.
    const indentFor = after => {
      const column = mode.indent(CM.copyState(mode, state), after, line);
      return column === undefined ? null : column;
    };
    const row = [indentFor(line.slice(lead)), indentFor('')];
    const stream = new CM.StringStream(line, TAB_SIZE, {
      lookAhead: n => lines[i + n],
      baseToken() {},
    });
    if (!stream.string && mode.blankLine) mode.blankLine(state);
    while (!stream.eol()) {
      const style = mode.token(stream, state);
      row.push(stream.start, stream.pos, styleId(style));
      stream.start = stream.pos;
    }
    out.push(row);
  }
  return out;
}

// mulberry32: a small seeded generator, so the fixture is reproducible.
function random(seed) {
  return () => {
    seed |= 0; seed = (seed + 0x6D2B79F5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const pieces = ['{', '}', '(', ')', '[', ']', '"', "'", '`', '/', '*', '/*',
  '*/', '//', '${', '=>', ':', ';', ',', '<', '>', '\\', '\n', ' ', '\t', 'x',
  'function ', 'class ', 'if ', 'else ', 'return ', '#', '@', '...', '?.',
  '??', '0x1F', '.5', '<!--', '-->', 'async ', 'type ', 'interface ', '=',
  '!', '++'];

function mutate(text, next) {
  const lines = text.split('\n');
  const from = Math.floor(next() * lines.length);
  const count = 1 + Math.floor(next() * 12);
  let window = lines.slice(from, from + count).join('\n');
  const edits = Math.floor(next() * 4);
  for (let e = 0; e < edits; e++) {
    const at = Math.floor(next() * (window.length + 1));
    const kind = next();
    if (kind < 0.4) {
      window = window.slice(0, at) + window.slice(at + 1 + Math.floor(next() * 5));
    } else if (kind < 0.9) {
      const piece = pieces[Math.floor(next() * pieces.length)];
      window = window.slice(0, at) + piece + window.slice(at);
    } else {
      window = window.slice(0, at);
    }
  }
  const layout = next();
  if (layout < 0.1) window = window.replace(/^( {2})+/gm, m => '\t'.repeat(m.length / 2));
  else if (layout < 0.15) window = window.replace(/\n/g, '\r\n');
  return window;
}

const corpusDir = otherCorpus || path.join(__dirname, '..', 'corpus');
const sources = fs.readdirSync(corpusDir).sort()
  .filter(name => modes[path.extname(name)])
  .map(name => ({
    name: `corpus/${name}`,
    mode: modes[path.extname(name)],
    text: fs.readFileSync(path.join(corpusDir, name), 'utf8'),
  }));
for (const file of otherCorpus ? [] : ['mode/javascript/javascript.js', 'addon/edit/closebrackets.js']) {
  sources.push({
    name: `codemirror/${file}`,
    mode: 'javascript',
    text: fs.readFileSync(path.join(cmDir, file), 'utf8'),
  });
}

const cases = [];
for (const source of sources) {
  cases.push(source);
  // The same text as TypeScript and JavaScript exercises both parsers.
  if (source.mode === 'javascript' && source.name.startsWith('corpus/')) {
    cases.push({ ...source, name: `${source.name} as typescript`, mode: 'typescript' });
  }
}
const next = random(20260924);
for (const source of sources) {
  for (let i = 0; i < (otherCorpus ? 3 : 60); i++) {
    cases.push({ name: `${source.name} mutation ${i}`, mode: source.mode, text: mutate(source.text, next) });
  }
}

const out = cases.map(c => {
  try {
    return { name: c.name, mode: c.mode, text: c.text, lines: run(c.text, c.mode) };
  } catch (e) {
    return { name: c.name, mode: c.mode, text: c.text, error: String(e && e.message || e) };
  }
});
fs.writeFileSync(outPath, JSON.stringify({
  codemirror: pkg.version,
  tabSize: TAB_SIZE,
  indentUnit: INDENT_UNIT,
  styles,
  cases: out,
}) + '\n');
console.log(`${out.length} cases, ${out.filter(c => c.error).length} errors, ${styles.length} styles`);
