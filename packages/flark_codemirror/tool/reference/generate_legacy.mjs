// Records CodeMirror 6's own tokens and indentation for the Dart ports of
// its legacy stream modes (@codemirror/legacy-modes), as generate.cjs does
// for CodeMirror 5. Needs Node and the two unpacked packages:
//
//   npm pack @codemirror/legacy-modes@6.5.4 @codemirror/language@6.12.4
//   (unpack each tarball into its own directory)
//   node tool/reference/generate_legacy.mjs <legacy-modes dir> <language dir> [language...]
//
// Languages come from legacy.json, each with its module and export; their
// cases are the files in tool/corpus/<language>/ and seeded mutations of
// short windows of them. Each language's fixture is written to
// test/fixtures/legacy/<language>.json. A further argument pair
// `--corpus <dir> --out <file>` runs one language over another directory
// for wider local checks.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..', '..');
const args = process.argv.slice(2);
const option = name => {
  const i = args.indexOf(name);
  if (i < 0) return null;
  const [value] = args.splice(i, 2).slice(1);
  return value;
};
const otherCorpus = option('--corpus'), otherOut = option('--out');
const [legacyDir, languageDir, ...only] = args;
if (!legacyDir || !languageDir) {
  console.error('usage: generate_legacy.mjs <legacy-modes dir> <language dir> [language...]');
  process.exit(2);
}
const version = dir => JSON.parse(fs.readFileSync(path.join(dir, 'package.json'), 'utf8')).version;
if (version(legacyDir) !== '6.5.4') throw new Error(`legacy-modes ${version(legacyDir)}`);
if (version(languageDir) !== '6.12.4') throw new Error(`language ${version(languageDir)}`);

// CodeMirror 6's StringStream, verbatim from @codemirror/language. Importing
// the package would need its other dependencies, so the class is lifted out.
const dist = fs.readFileSync(path.join(languageDir, 'dist/index.js'), 'utf8');
const lift = (from, to) => {
  const a = dist.indexOf(from), b = dist.indexOf(to, a);
  if (a < 0 || b < 0) throw new Error(`${from} not found in @codemirror/language`);
  return dist.slice(a, b);
};
const StringStream = new Function(
  `${lift('function countCol(', 'class StringStream')}
   ${lift('class StringStream', 'function fullParser(')}
   return StringStream;`)();

const TAB_SIZE = 4, INDENT_UNIT = 2;

// As @codemirror/language's StreamLanguage drives a stream parser.
function readToken(token, stream, state) {
  stream.start = stream.pos;
  for (let i = 0; i < 10; i++) {
    const result = token(stream, state);
    if (stream.pos > stream.start) return result;
  }
  throw new Error('Stream parser failed to advance stream.');
}
function defaultCopyState(state) {
  if (typeof state != 'object') return state;
  const copy = {};
  for (const prop in state) {
    const value = state[prop];
    copy[prop] = value instanceof Array ? value.slice() : value;
  }
  return copy;
}
// The indentation context: the ported modes read only its unit.
const context = new Proxy({unit: INDENT_UNIT}, {
  get(target, key) {
    if (key in target) return target[key];
    throw new Error(`the mode reads IndentContext.${String(key)}`);
  },
});

const styles = [null], styleIds = new Map([[null, 0]]);
function styleId(style) {
  const key = style === undefined ? null : style;
  if (!styleIds.has(key)) { styleIds.set(key, styles.length); styles.push(key); }
  return styleIds.get(key);
}

function run(parser, text) {
  const lines = text.split(/\r\n?|\n/);
  const copy = parser.copyState || defaultCopyState;
  const state = parser.startState ? parser.startState(INDENT_UNIT) : true;
  const out = [];
  for (const line of lines) {
    const lead = /^\s*/.exec(line)[0].length;
    const indentFor = after => {
      if (!parser.indent) return null;
      const column = parser.indent(copy(state), after, context);
      return column == null ? null : column;
    };
    const row = [indentFor(line.slice(lead)), indentFor('')];
    if (line.length) {
      const stream = new StringStream(line, TAB_SIZE, INDENT_UNIT);
      while (!stream.eol()) {
        const style = readToken(parser.token, stream, state);
        row.push(stream.start, stream.pos, styleId(style));
      }
    } else if (parser.blankLine) {
      parser.blankLine(state, INDENT_UNIT);
    }
    out.push(row);
  }
  return out;
}

// mulberry32, seeded per language so each fixture is reproducible alone.
function random(seed) {
  return () => {
    seed |= 0; seed = (seed + 0x6D2B79F5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const seedOf = name => [...name].reduce((h, c) => Math.imul(h ^ c.charCodeAt(0), 16777619), 2166136261);
const pieces = ['{', '}', '(', ')', '[', ']', '"', "'", '`', '"""', "'''", '#',
  '//', '/*', '*/', '--', '<!--', '-->', '\\', '\n', ' ', '\t', ':', ';', ',',
  '=', '$', '${', '$(', '@', '<', '>', '|', '&', '%', 'x', '0x1F', '.5', 'end',
  'def ', 'if ', 'then', 'do', 'fn ', 'func ', 'class ', 'return ', '<<', '*'];
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
      window = window.slice(0, at) + pieces[Math.floor(next() * pieces.length)] + window.slice(at);
    } else {
      window = window.slice(0, at);
    }
  }
  const layout = next();
  if (layout < 0.1) window = window.replace(/^( {2})+/gm, m => '\t'.repeat(m.length / 2));
  else if (layout < 0.15) window = window.replace(/\n/g, '\r\n');
  return window;
}

const manifest = JSON.parse(fs.readFileSync(path.join(here, 'legacy.json'), 'utf8'));
const names = otherCorpus ? only : only.length ? only : Object.keys(manifest);
for (const name of names) {
  const entry = manifest[name];
  if (!entry) throw new Error(`${name} is not in legacy.json`);
  const module = await import(pathToFileURL(path.join(legacyDir, 'mode', `${entry.module}.js`)).href);
  const parser = module[entry.export];
  if (!parser) throw new Error(`${entry.module}.js exports no ${entry.export}`);
  styles.length = 1; styleIds.clear(); styleIds.set(null, 0);
  const corpusDir = otherCorpus || path.join(root, 'tool', 'corpus', name);
  const sources = fs.readdirSync(corpusDir).sort()
    .filter(file => fs.statSync(path.join(corpusDir, file)).isFile())
    .map(file => ({name: `${name}/${file}`, text: fs.readFileSync(path.join(corpusDir, file), 'utf8')}));
  const cases = [...sources];
  const next = random(seedOf(name));
  const mutations = otherCorpus ? 3 : entry.mutations ?? 40;
  for (const source of sources) {
    for (let i = 0; i < mutations; i++) {
      cases.push({name: `${source.name} mutation ${i}`, text: mutate(source.text, next)});
    }
  }
  const out = cases.map(c => {
    try {
      return {name: c.name, text: c.text, lines: run(parser, c.text)};
    } catch (e) {
      return {name: c.name, text: c.text, error: String(e && e.message || e)};
    }
  });
  const file = otherOut || path.join(root, 'test', 'fixtures', 'legacy', `${name}.json`);
  fs.mkdirSync(path.dirname(file), {recursive: true});
  fs.writeFileSync(file, JSON.stringify({
    source: `@codemirror/legacy-modes ${version(legacyDir)} mode/${entry.module}.js ${entry.export}`,
    language: name,
    tabSize: TAB_SIZE,
    indentUnit: INDENT_UNIT,
    styles,
    cases: out,
  }) + '\n');
  console.log(`${name}: ${out.length} cases, ${out.filter(c => c.error).length} errors, ${styles.length} styles`);
}
