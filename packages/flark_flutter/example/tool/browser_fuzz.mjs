#!/usr/bin/env node
// Fuzzes the workbench's web build end to end in headless Chrome.
//
// Serves build/web, drives the page over the Chrome DevTools Protocol with
// real input (key events with text, editing commands, inserted text, IME
// compositions, mouse presses and drags, tab switches, accessibility), reads
// the document the workbench saves and the caret its input element holds
// after every event, and checks both against tool/browser_fuzz_oracle.dart,
// which replays the same logical edits on a FlarkEditor on the Dart VM.
// Failing sequences are minimized by replaying subsets in the browser.
//
//   flutter build web --wasm     # or: flutter build web
//   node tool/browser_fuzz.mjs --sequences 100 --seed 1 [--semantics]
//
// Options: --build <dir> (build/web), --sequences <n> (40), --steps <n>
// (30 events each), --seed <n>, --semantics (turn accessibility on after
// load, as a screen reader does), --only <seed:index> (one generated
// sequence), --replay <file.json> (one {doc, clipboard, events} sequence),
// --no-minimize, --out <file.jsonl> (failures), --verbose (trace events),
// --timeout <ms> (20000, per DevTools call), --debugger (keep the debugger
// attached to take a hung page's stack), --chrome <path>, --chrome-args
// "<args>". Requires node 22, Chrome and `dart` on the PATH.
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { existsSync, readFileSync, appendFileSync, mkdtempSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, extname, normalize, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const example = resolve(here, '..');

// ---------------------------------------------------------------- options --
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i < 0 ? fallback : args[i + 1];
};
const flag = (name) => args.includes(`--${name}`);
const options = {
  build: resolve(option('build', join(example, 'build', 'web'))),
  sequences: Number(option('sequences', 40)),
  steps: Number(option('steps', 30)),
  seed: Number(option('seed', Date.now() % 100000)),
  semantics: flag('semantics'),
  only: option('only', null),
  replay: option('replay', null),
  minimize: !flag('no-minimize'),
  out: option('out', null),
  chrome: option('chrome', defaultChrome()),
  chromeArgs: option('chrome-args', '').split(' ').filter(Boolean),
  verbose: flag('verbose'),
  timeout: Number(option('timeout', 20000)),
  // Keep the debugger attached, so that a hung page's stack can be taken:
  // it pauses a running loop, but is not enabled while one runs.
  debugger: flag('debugger'),
};

function defaultChrome() {
  if (process.platform === 'darwin') return '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
  for (const path of ['/usr/bin/google-chrome', '/usr/bin/chromium', '/usr/bin/chromium-browser']) {
    if (existsSync(path)) return path;
  }
  return 'google-chrome';
}

// ------------------------------------------------------------- utilities --
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (...parts) => console.log(...parts);
const debug = (...parts) => options.verbose && console.log(...parts);

/// A small seeded generator (mulberry32).
function random(seed) {
  let a = seed >>> 0;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const r = {
    next,
    int: (n) => Math.floor(next() * n),
    chance: (p) => next() < p,
    pick: (items) => items[Math.floor(next() * items.length)],
    weighted: (table) => {
      const total = table.reduce((sum, [w]) => sum + w, 0);
      let x = next() * total;
      for (const [w, value] of table) if ((x -= w) < 0) return value;
      return table[table.length - 1][1];
    },
  };
  return r;
}

const graphemes = (text) => [...new Intl.Segmenter(undefined, { granularity: 'grapheme' }).segment(text)].map((s) => s.segment);

// ---------------------------------------------------------------- server --
const types = {
  '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.wasm': 'application/wasm',
  '.json': 'application/json', '.png': 'image/png', '.otf': 'font/otf', '.ttf': 'font/ttf', '.frag': 'text/plain',
};

function serve(root) {
  const server = createServer((request, response) => {
    let path = normalize(decodeURIComponent(new URL(request.url, 'http://x').pathname)).replace(/^\/+/, '');
    if (path === '' || path.endsWith('/')) path += 'index.html';
    const file = join(root, path);
    if (!file.startsWith(root) || !existsSync(file) || statSync(file).isDirectory()) {
      response.writeHead(404);
      response.end();
      return;
    }
    response.writeHead(200, {
      'Content-Type': types[extname(file)] ?? 'application/octet-stream',
      // Cross-origin isolation lets skwasm use its render thread.
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
      'Cache-Control': path === 'index.html' ? 'no-store' : 'max-age=3600',
    });
    response.end(readFileSync(file));
  });
  return new Promise((r) => server.listen(0, '127.0.0.1', () => r(server)));
}

// ---------------------------------------------------------------- chrome --
async function launchChrome() {
  const profile = mkdtempSync(join(tmpdir(), 'flark-browser-fuzz-'));
  const chromeArgs = [
    '--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`,
    // A fresh profile would otherwise ask the macOS keychain, and wait.
    '--use-mock-keychain', '--no-first-run', '--no-default-browser-check',
    '--window-size=1200,900', ...options.chromeArgs, 'about:blank',
  ];
  // Under Rosetta (an Intel parent shell) Chrome starts as x86_64 and its
  // first translation takes minutes; ask for the native architecture.
  const native = process.platform === 'darwin' && process.arch === 'arm64';
  const child = native
    ? spawn('/usr/bin/arch', ['-arm64', options.chrome, ...chromeArgs], { stdio: 'ignore' })
    : spawn(options.chrome, chromeArgs, { stdio: 'ignore' });
  const exited = new Promise((r) => child.on('exit', r));
  let endpoint;
  for (let i = 0; i < 300 && !endpoint; i++) {
    await sleep(100);
    const file = join(profile, 'DevToolsActivePort');
    if (existsSync(file)) {
      const [port, path] = readFileSync(file, 'utf8').trim().split('\n');
      if (path) endpoint = `ws://127.0.0.1:${port}${path}`;
    }
  }
  if (!endpoint) {
    child.kill('SIGKILL');
    throw new Error('Chrome did not start');
  }
  return {
    endpoint,
    async close() {
      child.kill('SIGTERM');
      await Promise.race([exited, sleep(5000)]);
      if (child.exitCode === null) child.kill('SIGKILL');
      rmSync(profile, { recursive: true, force: true });
    },
  };
}

class Cdp {
  constructor(endpoint) {
    this.ws = new WebSocket(endpoint);
    this.next = 0;
    this.pending = new Map();
    this.listeners = [];
    this.ws.addEventListener('message', (event) => {
      const message = JSON.parse(event.data);
      if (message.id && this.pending.has(message.id)) {
        const { resolve, reject, method } = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (message.error) reject(new Error(`${method}: ${JSON.stringify(message.error)}`));
        else resolve(message.result);
      } else if (message.method) {
        for (const listener of this.listeners) listener(message);
      }
    });
  }
  open() {
    return new Promise((r) => this.ws.addEventListener('open', r, { once: true }));
  }
  send(method, params = {}, sessionId) {
    const id = ++this.next;
    this.ws.send(JSON.stringify({ id, method, params, sessionId }));
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`${method}: no reply in ${options.timeout} ms`));
      }, options.timeout);
      this.pending.set(id, {
        resolve: (value) => (clearTimeout(timer), resolve(value)),
        reject: (error) => (clearTimeout(timer), reject(error)),
        method,
      });
    });
  }
}

// ------------------------------------------------------------------ page --
const KEY = {
  Enter: { code: 'Enter', vk: 13, text: '\r' },
  Tab: { code: 'Tab', vk: 9 },
  Backspace: { code: 'Backspace', vk: 8 },
  Delete: { code: 'Delete', vk: 46 },
  Escape: { code: 'Escape', vk: 27 },
  ArrowLeft: { code: 'ArrowLeft', vk: 37 },
  ArrowUp: { code: 'ArrowUp', vk: 38 },
  ArrowRight: { code: 'ArrowRight', vk: 39 },
  ArrowDown: { code: 'ArrowDown', vk: 40 },
  Home: { code: 'Home', vk: 36 },
  End: { code: 'End', vk: 35 },
  PageUp: { code: 'PageUp', vk: 33 },
  PageDown: { code: 'PageDown', vk: 34 },
};
const MODIFIERS = [
  ['alt', 'Alt', 'AltLeft', 18, 1],
  ['ctrl', 'Control', 'ControlLeft', 17, 2],
  ['meta', 'Meta', 'MetaLeft', 91, 4],
  ['shift', 'Shift', 'ShiftLeft', 16, 8],
];
const PUNCTUATION = {
  ' ': ['Space', 32, false], '-': ['Minus', 189, false], '=': ['Equal', 187, false],
  '[': ['BracketLeft', 219, false], ']': ['BracketRight', 221, false], '\\': ['Backslash', 220, false],
  ';': ['Semicolon', 186, false], "'": ['Quote', 222, false], ',': ['Comma', 188, false],
  '.': ['Period', 190, false], '/': ['Slash', 191, false], '`': ['Backquote', 192, false],
  '!': ['Digit1', 49, true], '@': ['Digit2', 50, true], '#': ['Digit3', 51, true], $: ['Digit4', 52, true],
  '%': ['Digit5', 53, true], '^': ['Digit6', 54, true], '&': ['Digit7', 55, true], '*': ['Digit8', 56, true],
  '(': ['Digit9', 57, true], ')': ['Digit0', 48, true], _: ['Minus', 189, true], '+': ['Equal', 187, true],
  '{': ['BracketLeft', 219, true], '}': ['BracketRight', 221, true], '|': ['Backslash', 220, true],
  ':': ['Semicolon', 186, true], '"': ['Quote', 222, true], '<': ['Comma', 188, true],
  '>': ['Period', 190, true], '?': ['Slash', 191, true], '~': ['Backquote', 192, true],
};

/// macOS virtual key codes, which Chrome uses for the native event it
/// builds from a synthesized key.
const MAC_KEY_CODES = {
  ShiftLeft: 56, ControlLeft: 59, AltLeft: 58, MetaLeft: 55, KeyA: 0, KeyS: 1, KeyD: 2, KeyF: 3, KeyH: 4,
  KeyG: 5, KeyZ: 6, KeyX: 7, KeyC: 8, KeyV: 9, KeyB: 11, KeyQ: 12, KeyW: 13, KeyE: 14, KeyR: 15, KeyY: 16,
  KeyT: 17, Digit1: 18, Digit2: 19, Digit3: 20, Digit4: 21, Digit6: 22, Digit5: 23, Equal: 24, Digit9: 25,
  Digit7: 26, Minus: 27, Digit8: 28, Digit0: 29, BracketRight: 30, KeyO: 31, KeyU: 32, BracketLeft: 33,
  KeyI: 34, KeyP: 35, Enter: 36, KeyL: 37, KeyJ: 38, Quote: 39, KeyK: 40, Semicolon: 41, Backslash: 42,
  Comma: 43, Slash: 44, KeyN: 45, KeyM: 46, Period: 47, Tab: 48, Space: 49, Backquote: 50, Backspace: 51,
  Escape: 53, Delete: 117, Home: 115, End: 119, PageUp: 116, PageDown: 121, ArrowLeft: 123,
  ArrowRight: 124, ArrowDown: 125, ArrowUp: 126,
};

/// The macOS key bindings Chrome sends with a key as editing commands. The
/// browser runs them when the page does not prevent the key's default.
function macCommands(key, m) {
  const s = m.shift ? 'AndModifySelection' : '';
  if (m.meta) {
    return {
      ArrowLeft: [`moveToBeginningOfLine${s}`], ArrowRight: [`moveToEndOfLine${s}`],
      ArrowUp: [`moveToBeginningOfDocument${s}`], ArrowDown: [`moveToEndOfDocument${s}`],
      Backspace: ['deleteToBeginningOfLine'], Delete: ['deleteToEndOfLine'],
      a: ['selectAll'], c: ['copy'], x: ['cut'], v: ['paste'], z: [m.shift ? 'redo' : 'undo'],
    }[key] ?? [];
  }
  if (m.alt) {
    return {
      ArrowLeft: [`moveWordLeft${s}`], ArrowRight: [`moveWordRight${s}`],
      ArrowUp: [`moveToBeginningOfParagraph${s}`], ArrowDown: [`moveToEndOfParagraph${s}`],
      Backspace: ['deleteWordBackward'], Delete: ['deleteWordForward'],
    }[key] ?? [];
  }
  if (m.ctrl) return [];
  return {
    ArrowLeft: [`moveLeft${s}`], ArrowRight: [`moveRight${s}`], ArrowUp: [`moveUp${s}`], ArrowDown: [`moveDown${s}`],
    Backspace: ['deleteBackward'], Delete: ['deleteForward'], Enter: ['insertNewline'],
    Tab: [m.shift ? 'insertBacktab' : 'insertTab'], Escape: ['cancelOperation'],
    Home: ['scrollToBeginningOfDocument'], End: ['scrollToEndOfDocument'],
    PageUp: ['scrollPageUp'], PageDown: ['scrollPageDown'],
  }[key] ?? [];
}

class Page {
  constructor(url) {
    this.url = url;
    this.errors = [];
  }

  /// Starts a browser with this page. A hung renderer can take others of
  /// its site with it, so a restart replaces the whole browser.
  async start() {
    this.chrome = await launchChrome();
    this.cdp = new Cdp(this.chrome.endpoint);
    await this.cdp.open();
    this.cdp.listeners.push((message) => {
      if (message.sessionId !== this.session) return;
      if (message.method === 'Runtime.exceptionThrown') {
        const d = message.params.exceptionDetails;
        this.errors.push(`exception: ${d.exception?.description ?? d.text}`.slice(0, 600));
      } else if (message.method === 'Runtime.consoleAPICalled') {
        const text = message.params.args.map((a) => a.value ?? a.description ?? '').join(' ');
        if (message.params.type === 'error' || message.params.type === 'assert' || /exception|error|assert|══╡/i.test(text)) {
          this.errors.push(`console.${message.params.type}: ${text}`.slice(0, 600));
        }
      }
    });
    await this.open();
  }

  async close() {
    this.cdp?.ws.close();
    await this.chrome?.close();
    this.chrome = this.cdp = null;
  }

  async open() {
    const { targetId } = await this.cdp.send('Target.createTarget', { url: 'about:blank' });
    this.target = targetId;
    const { sessionId } = await this.cdp.send('Target.attachToTarget', { targetId, flatten: true });
    this.session = sessionId;
    await this.send('Runtime.enable');
    await this.send('Page.enable');
    if (options.debugger) await this.send('Debugger.enable');
    const origin = new URL(this.url).origin;
    await this.cdp.send('Browser.grantPermissions', { origin, permissions: ['clipboardReadWrite', 'clipboardSanitizedWrite'] });
    // Headless Chrome on macOS hands a synthesized key that the page leaves
    // unhandled, and that has no default action (a modifier, Escape,
    // PageDown), back to the page again and again: thousands of keydowns a
    // second, until nothing else gets a reply. A second keydown of a key
    // that is already down, and is not an autorepeat, is that echo. Consume
    // it before the page sees it; that ends the loop.
    await this.send('Page.addScriptToEvaluateOnNewDocument', {
      source: `(() => {
        const down = new Set();
        addEventListener('keydown', (e) => {
          if (down.has(e.code) && !e.repeat) {
            e.stopImmediatePropagation();
            e.preventDefault();
          }
          down.add(e.code);
        }, true);
        addEventListener('keyup', (e) => down.delete(e.code), true);
        addEventListener('blur', () => down.clear());
      })();`,
    });
    await this.send('Page.navigate', { url: this.url });
    await this.waitFor('document.readyState === "complete"', 20000);
  }

  /// Replaces a page that stopped answering.
  async reopen() {
    await this.close();
    await this.start();
  }

  send(method, params = {}) {
    return this.cdp.send(method, params, this.session);
  }

  async evaluate(expression) {
    const result = await this.send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw new Error(`evaluate: ${JSON.stringify(result.exceptionDetails).slice(0, 400)}`);
    return result.result.value;
  }

  async waitFor(expression, timeout = 15000) {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      try {
        if (await this.evaluate(expression)) return true;
      } catch {}
      await sleep(30);
    }
    throw new Error(`timed out waiting for ${expression}`);
  }

  /// Loads the workbench showing [doc] under the preset [name].
  async load(name, doc, clipboard) {
    await this.evaluate(`(() => {
      localStorage.clear();
      localStorage.setItem('flutter.v5.active', ${JSON.stringify(JSON.stringify(name))});
      localStorage.setItem(${JSON.stringify('flutter.v5.source.' + name)}, ${JSON.stringify(JSON.stringify(doc))});
      window.__fuzzOld = true;
      return true;
    })()`);
    this.key = `flutter.v5.source.${name}`;
    await this.send('Emulation.clearDeviceMetricsOverride');
    await this.reload();
    debug('  loaded');
    if (clipboard !== undefined) {
      const written = await this.evaluate(`navigator.clipboard.writeText(${JSON.stringify(clipboard)}).then(() => true, (e) => String(e))`);
      if (written !== true) throw new Error(`clipboard: ${written}`);
    }
    this.errors = [];
  }

  /// Reloads the page and waits for the editor to take input, then turns
  /// accessibility on as a screen reader would.
  async reload() {
    await this.evaluate('window.__fuzzOld = true');
    await this.send('Page.reload', { ignoreCache: false });
    await this.waitFor(`!window.__fuzzOld && document.readyState === 'complete' && !!document.querySelector('textarea.flt-text-editing') && document.activeElement === document.querySelector('textarea.flt-text-editing')`);
    await this.frames();
    if (options.semantics) {
      await this.evaluate(`(() => { const p = document.querySelector('flt-semantics-placeholder'); if (p) p.click(); return !!p; })()`);
      await this.waitFor(`!!document.querySelector('flt-semantics')`);
      await this.frames();
    }
  }

  /// Where the page's main thread is, when it stopped answering input.
  async stack() {
    const paused = new Promise((resolve) => {
      const listener = (message) => {
        if (message.sessionId === this.session && message.method === 'Debugger.paused') {
          this.cdp.listeners.splice(this.cdp.listeners.indexOf(listener), 1);
          resolve(message.params.callFrames);
        }
      };
      this.cdp.listeners.push(listener);
      setTimeout(() => resolve(null), 10000);
    });
    try {
      if (!options.debugger) await this.send('Debugger.enable');
      await this.send('Debugger.pause');
      const frames = await paused;
      if (!frames) return 'the page did not pause';
      const lines = frames.slice(0, 40).map((f) => `${f.functionName || '?'} ${f.url.split('/').pop()}:${f.location.lineNumber}:${f.location.columnNumber ?? 0}`);
      await this.send('Debugger.resume').catch(() => {});
      if (!options.debugger) await this.send('Debugger.disable').catch(() => {});
      return lines.join('\n');
    } catch (error) {
      return `no stack: ${error.message}`;
    }
  }

  frames() {
    return this.evaluate(`new Promise((r) => { setTimeout(() => r(false), 400); requestAnimationFrame(() => requestAnimationFrame(() => r(true))); })`);
  }

  async observe(extra = {}) {
    await this.frames();
    const state = await this.evaluate(`(() => {
      const el = document.querySelector('textarea.flt-text-editing');
      const saved = localStorage.getItem(${JSON.stringify(this.key)});
      return {
        value: el ? el.value : null,
        start: el ? el.selectionStart : null,
        end: el ? el.selectionEnd : null,
        focused: !!el && document.activeElement === el,
        saved: saved === null ? null : JSON.parse(saved),
      };
    })()`);
    const errors = this.errors;
    this.errors = [];
    return { ...state, ...extra, errors };
  }

  async keyEvent(type, key, code, vk, modifiers, text, commands) {
    const native = process.platform === 'darwin' ? MAC_KEY_CODES[code] ?? 0 : vk;
    await this.send('Input.dispatchKeyEvent', {
      type, key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: native, modifiers,
      // A modifier key is on one side of the keyboard. Without it, Flutter
      // reads no Shift, Alt or Meta as held.
      ...(['ShiftLeft', 'ControlLeft', 'AltLeft', 'MetaLeft'].includes(code) ? { location: 1 } : {}),
      ...(text === undefined ? {} : { text, unmodifiedText: text }),
      ...(commands && commands.length ? { commands } : {}),
    });
  }

  /// Presses [key] with modifiers held, as a keyboard does: each modifier
  /// goes down first and up last.
  async press(key, m = {}, { text, code, vk, commands } = {}) {
    let bits = 0;
    const held = MODIFIERS.filter(([name]) => m[name]);
    for (const [, k, c, v, bit] of held) {
      bits |= bit;
      await this.keyEvent('rawKeyDown', k, c, v, bits);
    }
    const spec = KEY[key] ?? {};
    const keyText = text ?? (m.meta || m.ctrl ? undefined : spec.text);
    await this.keyEvent(keyText === undefined ? 'rawKeyDown' : 'keyDown', key, code ?? spec.code ?? '', vk ?? spec.vk ?? 0, bits, keyText, commands ?? macCommands(key, m));
    await this.keyEvent('keyUp', key, code ?? spec.code ?? '', vk ?? spec.vk ?? 0, bits);
    for (const [, k, c, v, bit] of held.reverse()) {
      bits &= ~bit;
      await this.keyEvent('keyUp', k, c, v, bits);
    }
  }

  /// Types one grapheme as its key, with Shift for capitals and symbols.
  async typeKey(g) {
    if (/^[a-z]$/.test(g)) return this.press(g, {}, { text: g, code: `Key${g.toUpperCase()}`, vk: g.toUpperCase().charCodeAt(0) });
    if (/^[A-Z]$/.test(g)) return this.press(g, { shift: true }, { text: g, code: `Key${g}`, vk: g.charCodeAt(0), commands: [] });
    if (/^[0-9]$/.test(g)) return this.press(g, {}, { text: g, code: `Digit${g}`, vk: g.charCodeAt(0) });
    const p = PUNCTUATION[g];
    if (p) return this.press(g, p[2] ? { shift: true } : {}, { text: g, code: p[0], vk: p[1], commands: [] });
    // A character without a key of its own: a keyboard layout's, with the
    // key event's text, or, longer than a key's text can be, the character
    // viewer's.
    if (g.length > 2) return this.send('Input.insertText', { text: g });
    return this.press(g, {}, { text: g, code: '', vk: 0, commands: [] });
  }

  async mouse(type, x, y, { buttons = 0, clickCount = 0, modifiers = 0 } = {}) {
    await this.send('Input.dispatchMouseEvent', { type, x, y, button: type === 'mouseMoved' && !buttons ? 'none' : 'left', buttons, clickCount, modifiers });
  }
}

// ------------------------------------------------------------- generator --
const DOCS = [
  ['Draft', ''],
  ['Draft', 'Hello world'],
  ['Draft', '- one\n- two\n- three'],
  ['Draft', '1. first\n2. second\n3. third'],
  ['Draft', '- [ ] a task\n- [x] done task\n- plain'],
  ['Draft', '> a quote\n> continues here\n\nafter'],
  ['Draft', '> - quoted item\n> - another\n\n> > nested quote'],
  ['Draft', '# Heading\n\nA paragraph of text.\n\n## Second\n\nMore words here.'],
  ['Draft', '```js\nlet x = 1;\nconsole.log(x);\n```\n\nafter the fence'],
  ['Draft', '```\nplain code\n```'],
  ['Draft', '| a | b |\n| - | - |\n| 1 | 2 |\n| 3 | 4 |'],
  ['Draft', 'Emoji 👩🏽‍💻 and café é 日本語 😀 done'],
  ['Draft', 'line one\r\nline two\r\n\r\n- item\r\n- item two\r\n'],
  ['Draft', 'Some **bold** and *italic* and `code` and [a link](https://example.com) and ~~strike~~.'],
  ['Draft', '- outer\n  - inner\n    - deeper\n- back out'],
  ['Draft', 'Setext\n======\n\ntext under it'],
  ['Draft', 'a\n\n---\n\nb'],
  ['Draft', '***bold italic*** and **_mixed_** and `a*b*c`'],
  ['Draft', '<div>\nhtml block\n</div>\n\ntext'],
  ['Draft', '[ref]: https://example.com\n\nA [ref] link.'],
  ['Draft', '- one\r\n- two\r\n\r\n> quote\r\n'],
  ['Tour', null],
  ['Long line', null],
  ['Dense 16 KiB', null],
];

const WORDS = ['the', 'quick', 'brown', 'fox', 'a', 'markdown', 'note', 'list', 'Teh', 'and', 'word', 'x', 'ok'];
const SYMBOLS = ['*', '**', '_', '`', '#', '- ', '> ', '[', ']', '(', ')', '|', '~', '!', '1. ', '\\', '<', '>', '```', '---', '- [ ] '];
const UNICODE = ['é', 'ñ', '日', '本', '😀', '👍🏽', '👩‍💻', 'ü', 'ß', '中文', 'é'];
const CLIPBOARDS = ['pasted', 'two words', '**bold paste**', '- item\n- item', 'line\nbreak', '```\ncode\n```', '| x | y |', '😀 emoji', 'a\r\nb', '# heading', '> quoted'];
const IMES = [
  // [compositions..., committed]
  [['n', 'に', 'にほ', 'にほん', '日本'], '日本'],
  [['k', 'か'], 'か'],
  [['n', 'ni', '你'], '你'],
  [['ㅎ', '하', '한'], '한'],
  [['´'], 'é'],
  [['s', 'す', 'すし'], '寿司'],
  [['-'], '-'],
  [['*'], '*'],
  [['`'], '`'],
];

function generate(seed, index, steps) {
  const r = random(seed * 7919 + index * 104729 + 17);
  const [name, fixed] = r.pick(DOCS);
  const sequence = { id: `${seed}:${index}`, preset: name, doc: fixed, clipboard: r.pick(CLIPBOARDS), events: [] };
  const word = () => r.weighted([
    [6, () => r.pick(WORDS)],
    [2, () => r.pick(SYMBOLS)],
    [1, () => r.pick(UNICODE)],
    [1, () => String(r.int(100))],
  ])();
  // Presses land in the document: below the toolbar, which stays on one
  // line at these widths, and inside the window.
  let width = 1200;
  const point = () => ({ x: 30 + r.int(Math.min(700, width - 60)), y: 135 + r.int(r.chance(0.7) ? 260 : 620) });
  for (let i = 0; i < steps; i++) {
    const kind = r.weighted([
      [30, 'type'], [28, 'key'], [8, 'shortcut'], [4, 'insert'], [7, 'ime'],
      [6, 'click'], [2, 'dblclick'], [2, 'drag'], [2, 'away'], [2, 'pause'],
      [1, 'reload'], [1, 'resize'], [1, 'wheel'],
      // Toolbar buttons move with the document (a fence adds a language
      // menu); accessibility gives their places.
      ...(options.semantics ? [[2, 'semfocus'], [4, 'toolbar']] : []),
    ]);
    let event;
    switch (kind) {
      case 'type': {
        const text = Array.from({ length: 1 + r.int(3) }, word).join(r.chance(0.7) ? ' ' : '');
        event = { k: 'type', keys: graphemes(text + (r.chance(0.3) ? ' ' : '')) };
        break;
      }
      case 'key': {
        const key = r.weighted([
          [16, 'Enter'], [20, 'Backspace'], [6, 'Delete'], [10, 'ArrowLeft'], [10, 'ArrowRight'],
          [5, 'ArrowUp'], [5, 'ArrowDown'], [2, 'Home'], [2, 'End'], [5, 'Tab'], [2, 'Escape'], [1, 'PageDown'],
        ]);
        event = { k: 'key', key };
        const arrows = key.startsWith('Arrow');
        if ((arrows || key === 'Tab' || key === 'Enter') && r.chance(0.25)) event.shift = true;
        if ((arrows || key === 'Backspace' || key === 'Delete') && r.chance(0.15)) event.alt = true;
        else if (arrows && r.chance(0.12)) event.meta = true;
        break;
      }
      case 'shortcut':
        event = { k: 'shortcut', key: r.weighted([[4, 'z'], [2, 'y'], [2, 'b'], [2, 'i'], [2, 'a'], [2, 'c'], [2, 'x'], [3, 'v']]) };
        if (event.key === 'y') event = { k: 'shortcut', key: 'z', shift: true };
        break;
      case 'insert':
        event = { k: 'insert', text: r.weighted([[3, () => word()], [1, () => `${word()} ${word()}`], [1, () => `${word()}\n${word()}`], [1, () => r.pick(UNICODE)]])() };
        break;
      case 'ime': {
        const [steps, commit] = r.pick(IMES);
        const end = r.weighted([[14, 'commit'], [3, 'cancel'], [2, 'escape'], [1, 'enter']]);
        event = { k: 'ime', steps, end, commit };
        break;
      }
      case 'click':
        event = { k: 'click', ...point(), ...(r.chance(0.15) ? { shift: true } : {}) };
        break;
      case 'dblclick':
        event = { k: 'dblclick', ...point() };
        break;
      case 'drag': {
        const a = point(), b = point();
        event = { k: 'drag', x: a.x, y: a.y, x2: b.x, y2: b.y, backward: b.y < a.y - 10 || (Math.abs(b.y - a.y) <= 10 && b.x < a.x) };
        break;
      }
      case 'away':
        event = { k: 'away' };
        break;
      case 'pause':
        event = { k: 'pause' };
        break;
      case 'semfocus':
        event = { k: 'semfocus' };
        break;
      case 'reload':
        event = { k: 'reload' };
        break;
      case 'resize':
        width = r.pick([1200, 960, 800]);
        event = { k: 'resize', width };
        break;
      case 'wheel':
        event = { k: 'wheel', ...point(), delta: r.pick([-400, -120, 120, 400]) };
        break;
      case 'toolbar':
        event = { k: 'toolbar', button: r.weighted([[3, 'Bold'], [3, 'Italic'], [1, 'Strikethrough'], [1, 'Inline code'], [2, 'Undo'], [1, 'Redo'], [1, 'Source']]) };
        break;
    }
    sequence.events.push(event);
  }
  return sequence;
}

// ---------------------------------------------------------------- runner --
async function runEvent(page, event, clock, trace = () => {}) {
  const t = () => clock.now();
  switch (event.k) {
    case 'type': {
      const times = [];
      for (const g of event.keys) {
        times.push(t());
        trace(`    key ${JSON.stringify(g)}`);
        await page.typeKey(g);
      }
      return { t: times };
    }
    case 'key': {
      const at = t();
      await page.press(event.key, event);
      return { t: at };
    }
    case 'shortcut': {
      const at = t();
      const vk = event.key.toUpperCase().charCodeAt(0);
      await page.press(event.key, { meta: true, shift: event.shift }, { code: `Key${event.key.toUpperCase()}`, vk });
      return { t: at };
    }
    case 'insert': {
      const at = t();
      await page.send('Input.insertText', { text: event.text });
      return { t: at };
    }
    case 'ime': {
      const at = t();
      for (const text of event.steps) {
        await page.send('Input.imeSetComposition', { text, selectionStart: text.length, selectionEnd: text.length });
        await page.frames();
      }
      const commitAt = t();
      switch (event.end) {
        case 'commit':
          await page.send('Input.insertText', { text: event.commit });
          break;
        case 'enter':
          // An input method commits on Return. The page sees the key while
          // composing, as key code 229, and no text from it.
          await page.keyEvent('rawKeyDown', 'Enter', 'Enter', 229, 0);
          await page.send('Input.insertText', { text: event.commit });
          await page.keyEvent('keyUp', 'Enter', 'Enter', 13, 0);
          break;
        case 'cancel':
          await page.send('Input.imeSetComposition', { text: '', selectionStart: 0, selectionEnd: 0 });
          break;
        case 'escape':
          await page.press('Escape', {}, { commands: [] });
          await page.send('Input.imeSetComposition', { text: '', selectionStart: 0, selectionEnd: 0 });
          break;
      }
      return { t: commitAt, began: at };
    }
    case 'click': {
      const at = t();
      const modifiers = event.shift ? 8 : 0;
      if (event.shift) await page.keyEvent('rawKeyDown', 'Shift', 'ShiftLeft', 16, 8);
      await page.mouse('mouseMoved', event.x, event.y, { modifiers });
      await page.mouse('mousePressed', event.x, event.y, { buttons: 1, clickCount: 1, modifiers });
      await page.mouse('mouseReleased', event.x, event.y, { clickCount: 1, modifiers });
      if (event.shift) await page.keyEvent('keyUp', 'Shift', 'ShiftLeft', 16, 0);
      return { t: at };
    }
    case 'dblclick': {
      const at = t();
      await page.mouse('mouseMoved', event.x, event.y);
      for (const clickCount of [1, 2]) {
        await page.mouse('mousePressed', event.x, event.y, { buttons: 1, clickCount });
        await page.mouse('mouseReleased', event.x, event.y, { clickCount });
      }
      return { t: at };
    }
    case 'drag': {
      const at = t();
      await page.mouse('mouseMoved', event.x, event.y);
      await page.mouse('mousePressed', event.x, event.y, { buttons: 1, clickCount: 1 });
      for (let i = 1; i <= 5; i++) {
        await page.mouse('mouseMoved', event.x + ((event.x2 - event.x) * i) / 5, event.y + ((event.y2 - event.y) * i) / 5, { buttons: 1 });
      }
      await page.mouse('mouseReleased', event.x2, event.y2, { clickCount: 1 });
      return { t: at };
    }
    case 'away': {
      // Another tab takes the window's focus and hides the page, then the
      // user comes back.
      const at = t();
      const { targetId } = await page.cdp.send('Target.createTarget', { url: 'about:blank' });
      await sleep(250);
      await page.cdp.send('Target.activateTarget', { targetId: page.target });
      await page.cdp.send('Target.closeTarget', { targetId });
      await sleep(150);
      return { t: at };
    }
    case 'pause':
      await sleep(1400);
      return { t: t() };
    case 'reload': {
      // The draft must survive; the editor starts again at its first caret.
      const at = t();
      await page.reload();
      return { t: at };
    }
    case 'resize': {
      const at = t();
      await page.send('Emulation.setDeviceMetricsOverride', { width: event.width, height: 813, deviceScaleFactor: 1, mobile: false });
      await page.frames();
      return { t: at };
    }
    case 'wheel': {
      const at = t();
      await page.send('Input.dispatchMouseEvent', { type: 'mouseWheel', x: event.x, y: event.y, deltaX: 0, deltaY: event.delta });
      await page.frames();
      return { t: at };
    }
    case 'toolbar': {
      const at = t();
      const point = await page.evaluate(`(() => {
        const button = [...document.querySelectorAll('flt-semantics[role="button"]')]
          .find((n) => n.textContent === ${JSON.stringify(event.button)});
        if (!button) return null;
        const r = button.getBoundingClientRect();
        return [r.x + r.width / 2, r.y + r.height / 2];
      })()`);
      if (!point) return { t: at, missing: true };
      await page.mouse('mouseMoved', point[0], point[1]);
      await page.mouse('mousePressed', point[0], point[1], { buttons: 1, clickCount: 1 });
      await page.mouse('mouseReleased', point[0], point[1], { clickCount: 1 });
      return { t: at };
    }
    case 'semfocus': {
      // An assistive technology moves focus to the editor's text field node.
      const at = t();
      await page.evaluate(`(() => { const s = document.querySelector('flt-semantics textarea, flt-semantics input'); if (s) s.focus(); return !!s; })()`);
      return { t: at };
    }
  }
  throw new Error(`unknown event ${event.k}`);
}

/// Runs [sequence] (its events possibly a subset) and returns the record
/// the oracle checks.
async function runSequence(page, sequence, presets) {
  const doc = sequence.doc ?? presets[sequence.preset];
  debug(`load ${sequence.id} ${sequence.preset}`);
  await page.load(sequence.preset, doc, sequence.clipboard);
  const started = Date.now();
  const clock = { now: () => Date.now() - started };
  const record = { id: sequence.id, doc, clipboard: sequence.clipboard, initial: await page.observe(), events: [] };
  let lastCommand = 0;
  for (const [index, event] of sequence.events.entries()) {
    // History joins typing less than a second apart. Keep the gaps between
    // edits clear of that edge, so that the oracle's clock groups the same.
    const gap = clock.now() - lastCommand;
    if (gap > 600 && gap < 1400) await sleep(1400 - gap);
    debug(`  ${JSON.stringify(event)}`);
    let timing;
    try {
      timing = await runEvent(page, event, clock, debug);
    } catch (error) {
      // The events that ran still have to match.
      error.record = record;
      error.step = index;
      throw error;
    }
    if (!['away', 'pause', 'semfocus', 'reload', 'resize', 'wheel'].includes(event.k)) lastCommand = clock.now();
    const obs = await page.observe({ expectInput: event.k !== 'semfocus' });
    debug(`    -> ${JSON.stringify(obs.saved)} ${obs.start}..${obs.end}`);
    record.events.push({ event, ...timing, obs });
  }
  return record;
}

// ---------------------------------------------------------------- oracle --
class Oracle {
  constructor() {
    this.child = spawn('dart', ['run', 'tool/browser_fuzz_oracle.dart'], { cwd: example, stdio: ['pipe', 'pipe', 'inherit'] });
    this.buffer = '';
    this.waiting = [];
    this.child.stdout.setEncoding('utf8');
    this.child.stdout.on('data', (chunk) => {
      this.buffer += chunk;
      let newline;
      while ((newline = this.buffer.indexOf('\n')) >= 0) {
        const line = this.buffer.slice(0, newline);
        this.buffer = this.buffer.slice(newline + 1);
        // `dart run` may print its progress on the same line.
        const json = line.indexOf('{"');
        if (json < 0) continue;
        this.waiting.shift()?.(JSON.parse(line.slice(json)));
      }
    });
  }
  ask(request) {
    return new Promise((resolve) => {
      this.waiting.push(resolve);
      this.child.stdin.write(JSON.stringify(request) + '\n');
    });
  }
  close() {
    this.child.stdin.end();
  }
}

/// Runs [sequence] and asks the oracle. A page that stops answering input
/// is reported with its main thread's stack and replaced.
async function attempt(page, oracle, presets, sequence) {
  try {
    const record = await runSequence(page, sequence, presets);
    return { verdict: await oracle.ask({ sequence: record }), events: record.events.length };
  } catch (error) {
    const verdict = { ok: false, step: error.step ?? sequence.events.length, why: `driver: ${error.message}` };
    if (/no reply/.test(error.message)) {
      verdict.why = `hang: ${error.message}`;
      verdict.event = JSON.stringify(sequence.events[verdict.step]);
      verdict.stack = await page.stack();
      await page.reopen();
    }
    if (error.record) {
      const prefix = await oracle.ask({ sequence: error.record });
      if (!prefix.ok) return { verdict: prefix, events: error.record.events.length };
      verdict.expected = prefix.final;
    }
    return { verdict, events: error.record?.events.length ?? 0 };
  }
}

const category = (verdict) => String(verdict.why).split(':')[0];

// -------------------------------------------------------------- minimize --
async function minimize(page, oracle, presets, sequence, failure) {
  const fails = async (events) => {
    const { verdict } = await attempt(page, oracle, presets, { ...sequence, events });
    return !verdict.ok && category(verdict) === category(failure) ? verdict : null;
  };
  // Events after the first mismatch cannot have caused it.
  let events = sequence.events.slice(0, Math.max(0, failure.step) + 1);
  let verdict = (await fails(events)) ?? failure;
  let chunk = Math.max(1, Math.floor(events.length / 2));
  let tries = 0;
  while (chunk >= 1 && tries < 80) {
    let removed = false;
    for (let at = 0; at < events.length && tries < 80; at += chunk) {
      const candidate = [...events.slice(0, at), ...events.slice(at + chunk)];
      if (!candidate.length) continue;
      tries++;
      const result = await fails(candidate);
      if (result) {
        events = candidate.slice(0, Math.max(0, result.step) + 1);
        verdict = result;
        removed = true;
        at -= chunk;
      }
    }
    if (!removed) chunk = Math.floor(chunk / 2);
  }
  return { events, verdict };
}

// ------------------------------------------------------------------ main --
async function main() {
  if (!existsSync(join(options.build, 'index.html'))) throw new Error(`no web build at ${options.build}`);
  const server = await serve(options.build);
  const url = `http://127.0.0.1:${server.address().port}/`;
  const oracle = new Oracle();
  const page = new Page(url);
  // Interrupted, still stop Chrome and the oracle.
  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.once(signal, async () => {
      oracle.child.kill();
      await page.close();
      process.exit(130);
    });
  }
  let failures = 0;
  try {
    await page.start();
    const { presets } = await oracle.ask({ presets: true });
    const build = existsSync(join(options.build, 'main.dart.wasm')) && !options.build.endsWith('_js') ? 'wasm' : 'js';
    log(`browser fuzz: ${options.build} (${build}), seed ${options.seed}, semantics ${options.semantics}`);
    const indices = options.only || options.replay ? [Number(options.only?.split(':')[1] ?? 0)] : [...Array(options.sequences).keys()];
    const seed = options.only ? Number(options.only.split(':')[0]) : options.seed;
    let ran = 0, events = 0;
    const unchecked = {};
    for (const index of indices) {
      // A replay file holds one sequence: {preset, doc, clipboard, events}.
      const sequence = options.replay
        ? { id: 'replay', preset: 'Draft', clipboard: '', ...JSON.parse(readFileSync(options.replay, 'utf8')) }
        : generate(seed, index, options.steps);
      const result = await attempt(page, oracle, presets, sequence);
      const verdict = result.verdict;
      events += result.events;
      if (verdict.stack) log(`  page stack:\n${verdict.stack}`);
      ran++;
      if (verdict.ok) {
        // From a click in a long line of one repeated word, or a known
        // issue, the oracle cannot predict the page; it checks only errors.
        const why = verdict.unchecked?.why;
        if (why) unchecked[why] = (unchecked[why] ?? 0) + 1;
        debug(`ok ${sequence.id} (${sequence.preset})${why ? ` (unchecked from step ${verdict.unchecked.step}: ${why})` : ''}`);
        continue;
      }
      failures++;
      log(`FAIL ${sequence.id} (${sequence.preset}) at step ${verdict.step}: ${verdict.why}`);
      let minimized = null;
      if (options.minimize && category(verdict) !== 'driver') {
        minimized = await minimize(page, oracle, presets, sequence, verdict);
        log(`  minimized to ${minimized.events.length} events: ${JSON.stringify(minimized.events)}`);
        log(`  ${minimized.verdict.why} after ${minimized.verdict.event}`);
        log(`  expected ${JSON.stringify(minimized.verdict.expected)}`);
        log(`  observed ${JSON.stringify(minimized.verdict.observed)}`);
      } else {
        log(`  expected ${JSON.stringify(verdict.expected)}`);
        log(`  observed ${JSON.stringify(verdict.observed)}`);
      }
      if (options.out) {
        appendFileSync(options.out, JSON.stringify({ build, semantics: options.semantics, sequence, verdict, minimized }) + '\n');
      }
    }
    const partly = Object.entries(unchecked).map(([why, n]) => `${n} ${why}`).join(', ');
    log(`browser fuzz: ${ran} sequences, ${events} events, ${failures} failing (${build}, semantics ${options.semantics}, seed ${seed})${partly ? `; checked in part: ${partly}` : ''}`);
  } finally {
    oracle.close();
    await page.close();
    server.close();
  }
  process.exitCode = failures ? 1 : 0;
}

await main();
