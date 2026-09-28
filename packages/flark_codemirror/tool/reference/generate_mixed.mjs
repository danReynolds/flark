// Records tokens and indentation for Flark's composed modes, from the same
// parts the Dart ports are made of:
// - html: CodeMirror 5.65.21's htmlmixed logic over @codemirror/legacy-modes
//   6.5.4's xml (html) and css modes and CodeMirror 5.65.21's javascript mode;
// - php: CodeMirror 5.65.21's php logic and PHP configuration over that html
//   mode and @codemirror/legacy-modes 6.5.4's clike engine.
// Adaptations to the CodeMirror 6 modes, the same in the Dart ports: the
// xml mode styles a tag's angle brackets `angleBracket` where CodeMirror 5
// styled them `tag bracket`; the css and clike modes take the enclosing html
// indentation as their top context's, as CodeMirror 5's startState(base)
// did; and a PHP fence starts in PHP, with `<?php` styled meta, unless it
// opens with an HTML tag, since fences usually hold bare PHP.
//
//   node tool/reference/generate_mixed.mjs <legacy-modes dir> <language dir> <codemirror 5 dir> [language]
//
// Cases are the files in tool/corpus/html-mixed/ and tool/corpus/php/ with
// seeded mutations; the fixtures are test/fixtures/mixed/<language>.json.
// For wider local checks, `--corpus <dir> --out <file>` runs one language
// over another directory, and `--mutations <n>` sets the mutations per file.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
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
const mutationCount = Number(option('--mutations') ?? (otherCorpus ? 3 : 40));
const [legacyDir, languageDir, cm5Dir, only] = args;
if (!legacyDir || !languageDir || !cm5Dir || (otherCorpus && !only)) {
  console.error('usage: generate_mixed.mjs <legacy-modes dir> <language dir> <codemirror 5 dir> [language]');
  process.exit(2);
}
const version = dir => JSON.parse(fs.readFileSync(path.join(dir, 'package.json'), 'utf8')).version;
if (version(legacyDir) !== '6.5.4') throw new Error(`legacy-modes ${version(legacyDir)}`);
if (version(languageDir) !== '6.12.4') throw new Error(`language ${version(languageDir)}`);
if (version(cm5Dir) !== '5.65.21') throw new Error(`codemirror ${version(cm5Dir)}`);

// CodeMirror 6's StringStream, lifted from @codemirror/language as
// generate_legacy.mjs does.
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
const context = {unit: INDENT_UNIT};
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

const legacy = async name => import(pathToFileURL(path.join(legacyDir, 'mode', `${name}.js`)).href);
const {html: xmlHtml} = await legacy('xml');
const {css: cssParser} = await legacy('css');
const {clike} = await legacy('clike');
const require = createRequire(import.meta.url);
const CM5 = require(path.join(cm5Dir, 'addon/runmode/runmode.node.js'));
require(path.join(cm5Dir, 'mode/javascript/javascript.js'));
// php.js registers its PHP configuration, hooks included, as text/x-php.
require(path.join(cm5Dir, 'mode/php/php.js'));
const phpConfig = CM5.mimeModes['text/x-php'];
const javascript = CM5.getMode({indentUnit: INDENT_UNIT, tabSize: TAB_SIZE}, 'javascript');

// Inner modes behind one interface: CodeMirror 6 stream parsers take the
// indentation context, CodeMirror 5 modes the line.
const inner = {
  javascript: {
    start: base => javascript.startState(base),
    token: (stream, state) => javascript.token(stream, state),
    indent: (state, after, line) => {
      const column = javascript.indent(state, after, line);
      return column === undefined ? null : column;
    },
    copy: state => CM5.copyState(javascript, state),
  },
  css: {
    start: base => {
      const state = cssParser.startState(INDENT_UNIT);
      state.context.indent = base || 0;
      return state;
    },
    token: (stream, state) => cssParser.token(stream, state),
    indent: (state, after) => cssParser.indent(state, after, context),
    copy: state => (cssParser.copyState || defaultCopyState)(state),
  },
  'text/plain': {
    start: () => true,
    token: stream => { stream.skipToEnd(); },
    indent: null,
    copy: state => state,
  },
};
const html = {
  start: () => xmlHtml.startState(INDENT_UNIT),
  token: (stream, state) => xmlHtml.token(stream, state),
  indent: (state, after) => xmlHtml.indent(state, after, context),
  copy: state => (xmlHtml.copyState || defaultCopyState)(state),
};

// htmlmixed.js, with the angle bracket adaptation.
const tags = {
  script: [
    ['lang', /(javascript|babel)/i, 'javascript'],
    ['type', /^(?:text|application)\/(?:x-)?(?:java|ecma)script$|^module$|^$/i, 'javascript'],
    ['type', /./, 'text/plain'],
    [null, null, 'javascript'],
  ],
  style: [
    ['lang', /^css$/i, 'css'],
    ['type', /^(text\/)?(x-)?(stylesheet|css)$/i, 'css'],
    ['type', /./, 'text/plain'],
    [null, null, 'css'],
  ],
};
function maybeBackup(stream, pat, style) {
  const cur = stream.current(), close = cur.search(pat);
  if (close > -1) {
    stream.backUp(cur.length - close);
  } else if (cur.match(/<\/?$/)) {
    stream.backUp(cur.length);
    if (!stream.match(pat, false)) stream.match(cur);
  }
  return style;
}
const attrRegexps = {};
function getAttrValue(text, attr) {
  const regexp = attrRegexps[attr] ||
    (attrRegexps[attr] = new RegExp('\\s+' + attr + '\\s*=\\s*(\'|")?([^\'"]+)(\'|")?\\s*'));
  const match = text.match(regexp);
  return match ? /^\s*(.*?)\s*$/.exec(match[2])[1] : '';
}
const getTagRegexp = (tagName, anchored) =>
  new RegExp((anchored ? '^' : '') + '<\/\\s*' + tagName + '\\s*>', 'i');
function findMatchingMode(tagInfo, tagText) {
  for (const spec of tagInfo) {
    if (!spec[0] || spec[1].test(getAttrValue(tagText, spec[0]))) return spec[2];
  }
}
function htmlToken(stream, state) {
  const style = html.token(stream, state.htmlState);
  const tag = /\btag\b/.test(style) || style == 'angleBracket';
  let tagName;
  if (tag && !/[<>\s\/]/.test(stream.current()) &&
      (tagName = state.htmlState.tagName && state.htmlState.tagName.toLowerCase()) &&
      tags.hasOwnProperty(tagName)) {
    state.inTag = tagName + ' ';
  } else if (state.inTag && tag && />$/.test(stream.current())) {
    const inTag = /^([\S]+) (.*)/.exec(state.inTag);
    state.inTag = null;
    const modeSpec = stream.current() == '>' && findMatchingMode(tags[inTag[1]], inTag[2]);
    const mode = inner[modeSpec] || inner['text/plain'];
    const endTagA = getTagRegexp(inTag[1], true), endTag = getTagRegexp(inTag[1], false);
    state.token = (stream, state) => {
      if (stream.match(endTagA, false)) {
        state.token = htmlToken;
        state.localState = state.localMode = null;
        return null;
      }
      return maybeBackup(stream, endTag, state.localMode.token(stream, state.localState));
    };
    state.localMode = mode;
    state.localState = mode.start(html.indent(state.htmlState, ''));
  } else if (state.inTag) {
    state.inTag += stream.current();
    if (stream.eol()) state.inTag += ' ';
  }
  return style;
}
const mixed = {
  startState: () => ({token: htmlToken, inTag: null, localMode: null, localState: null, htmlState: html.start()}),
  copyState: state => ({
    token: state.token, inTag: state.inTag, localMode: state.localMode,
    localState: state.localState ? state.localMode.copy(state.localState) : undefined,
    htmlState: html.copy(state.htmlState),
  }),
  token: (stream, state) => state.token(stream, state),
  indent: (state, textAfter, line) => {
    if (!state.localMode || /^\s*<\//.test(textAfter)) return html.indent(state.htmlState, textAfter);
    if (state.localMode.indent) return state.localMode.indent(state.localState, textAfter, line);
    return null;
  },
};

// php.js's php mode over the mixed html mode, with the fence adaptation.
const phpParser = clike(phpConfig);
const php = {
  start: base => {
    const state = phpParser.startState(INDENT_UNIT);
    state.context.indented = (base || 0) - INDENT_UNIT;
    return state;
  },
  token: (stream, state) => phpParser.token(stream, state),
  indent: (state, after) => phpParser.indent(state, after, context),
  copy: state => (phpParser.copyState || defaultCopyState)(state),
};
function phpDispatch(stream, state) {
  if (!state.sniffed) {
    state.sniffed = true;
    if (/^\s*<(?!\?)/.test(stream.string)) {
      state.curMode = 'html';
      state.curState = state.html;
      state.php = null;
    }
  }
  const isPHP = state.curMode == 'php';
  if (stream.sol() && state.pending && state.pending != '"' && state.pending != "'") state.pending = null;
  if (!isPHP) {
    if (stream.match(/^<\?\w*/)) {
      state.curMode = 'php';
      if (!state.php) state.php = php.start(mixed.indent(state.html, '', ''));
      state.curState = state.php;
      return 'meta';
    }
    let style;
    if (state.pending == '"' || state.pending == "'") {
      while (!stream.eol() && stream.next() != state.pending) {}
      style = 'string';
    } else if (state.pending && stream.pos < state.pending.end) {
      stream.pos = state.pending.end;
      style = state.pending.style;
    } else {
      style = mixed.token(stream, state.curState);
    }
    if (state.pending) state.pending = null;
    const cur = stream.current(), openPHP = cur.search(/<\?/);
    let m;
    if (openPHP != -1) {
      if (style == 'string' && (m = cur.match(/[\'\"]$/)) && !/\?>/.test(cur)) state.pending = m[0];
      else state.pending = {end: stream.pos, style: style};
      stream.backUp(cur.length - openPHP);
    }
    return style;
  } else if (state.php.tokenize == null && stream.match('?>')) {
    state.curMode = 'html';
    state.curState = state.html;
    if (!state.php.context.prev) state.php = null;
    return 'meta';
  } else if (state.php.tokenize == null && stream.match(/^<\?\w*/)) {
    return 'meta';
  } else {
    return php.token(stream, state.curState);
  }
}
const phpMixed = {
  startState: () => {
    const phpState = php.start(0);
    return {html: mixed.startState(), php: phpState, curMode: 'php', curState: phpState, pending: null, sniffed: false};
  },
  copyState: state => {
    const html = mixed.copyState(state.html);
    const phpState = state.php && php.copy(state.php);
    return {html, php: phpState, curMode: state.curMode, curState: state.curMode == 'php' ? phpState : html,
      pending: state.pending, sniffed: state.sniffed};
  },
  token: phpDispatch,
  indent: (state, textAfter, line) => {
    if ((state.curMode != 'php' && /^\s*<\//.test(textAfter)) ||
        (state.curMode == 'php' && /^\?>/.test(textAfter))) {
      return mixed.indent(state.html, textAfter, line);
    }
    return state.curMode == 'php'
      ? php.indent(state.curState, textAfter)
      : mixed.indent(state.curState, textAfter, line);
  },
};

let styles, styleIds;
function styleId(style) {
  const key = style === undefined ? null : style;
  if (!styleIds.has(key)) { styleIds.set(key, styles.length); styles.push(key); }
  return styleIds.get(key);
}
function run(mode, text) {
  const lines = text.split(/\r\n?|\n/);
  const state = mode.startState();
  const out = [];
  for (const line of lines) {
    const lead = /^\s*/.exec(line)[0].length;
    const indentFor = after => {
      const column = mode.indent(mode.copyState(state), after, line);
      return column == null ? null : column;
    };
    const row = [indentFor(line.slice(lead)), indentFor('')];
    if (line.length) {
      const stream = new StringStream(line, TAB_SIZE, INDENT_UNIT);
      while (!stream.eol()) {
        const style = readToken(mode.token, stream, state);
        row.push(stream.start, stream.pos, styleId(style));
      }
    }
    out.push(row);
  }
  return out;
}

function random(seed) {
  return () => {
    seed |= 0; seed = (seed + 0x6D2B79F5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const pieces = ['<', '>', '</', '/>', '<script>', '</script>', '<style>', '</style>',
  '<!--', '-->', '"', "'", '=', '{', '}', '(', ')', ';', ':', '&amp;', '\n', ' ',
  '\t', 'x', '<div', '<p>', '</p>', '`', '/*', '*/', '//', '<?php', '?>', '<?=',
  '$', '->', '#', '<<<EOT', '${', '{$'];
function mutate(text, next) {
  const lines = text.split('\n');
  const from = Math.floor(next() * lines.length);
  let window = lines.slice(from, from + 1 + Math.floor(next() * 12)).join('\n');
  for (let e = Math.floor(next() * 4); e > 0; e--) {
    const at = Math.floor(next() * (window.length + 1)), kind = next();
    if (kind < 0.4) window = window.slice(0, at) + window.slice(at + 1 + Math.floor(next() * 5));
    else if (kind < 0.9) window = window.slice(0, at) + pieces[Math.floor(next() * pieces.length)] + window.slice(at);
    else window = window.slice(0, at);
  }
  return window;
}

function generate(language, corpus, mode, seed) {
  styles = [null];
  styleIds = new Map([[null, 0]]);
  if (only && only !== language) return;
  const corpusDir = otherCorpus || path.join(root, 'tool', 'corpus', corpus);
  const sources = fs.readdirSync(corpusDir).sort()
    .filter(file => fs.statSync(path.join(corpusDir, file)).isFile())
    .map(file => ({name: `${corpus}/${file}`, text: fs.readFileSync(path.join(corpusDir, file), 'utf8')}));
  const cases = [...sources];
  const next = random(seed);
  for (const source of sources) {
    for (let i = 0; i < mutationCount; i++) cases.push({name: `${source.name} mutation ${i}`, text: mutate(source.text, next)});
  }
  const out = cases.map(c => {
    try {
      return {name: c.name, text: c.text, lines: run(mode, c.text)};
    } catch (e) {
      return {name: c.name, text: c.text, error: String(e && e.message || e)};
    }
  });
  const file = otherOut || path.join(root, 'test', 'fixtures', 'mixed', `${language}.json`);
  fs.mkdirSync(path.dirname(file), {recursive: true});
  fs.writeFileSync(file, JSON.stringify({
    source: language == 'html'
      ? 'codemirror 5.65.21 htmlmixed over @codemirror/legacy-modes 6.5.4 xml (html) and css and codemirror 5.65.21 javascript'
      : 'codemirror 5.65.21 php over that html mode and @codemirror/legacy-modes 6.5.4 clike',
    language,
    tabSize: TAB_SIZE,
    indentUnit: INDENT_UNIT,
    styles,
    cases: out,
  }) + '\n');
  console.log(`${language}: ${out.length} cases, ${out.filter(c => c.error).length} errors, ${styles.length} styles`);
}

generate('html', 'html-mixed', mixed, 20260927);
generate('php', 'php', phpMixed, 20260928);
