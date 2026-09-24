/// Try `flark_codemirror`, CodeMirror 5's JavaScript mode ported to Dart, on
/// TypeScript in the Flutter editor, with the current Tree-sitter engine one
/// click away for comparison:
///
/// ```sh
/// flutter build web --wasm -t lib/codemirror_demo.dart
/// python3 tool/serve_web.py
/// ```
///
/// CodeMirror colors each fence synchronously on the UI thread; Tree-sitter
/// runs as the workbench does, with colors from a worker. The bar shows how
/// long the last uncached fence highlight and the last Enter/typing proposal
/// took on the UI thread.
library;

import 'package:flark/code.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'package:flark_flutter/code.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/material.dart';

import 'backend.dart';
import 'qualification.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(_Demo(await loadBackend()));
}

enum _Engine { codeMirror, treeSitter }

class _Demo extends StatefulWidget {
  const _Demo(this.backend);
  final FlarkParseBackend backend;

  @override
  State<_Demo> createState() => _DemoState();
}

class _DemoState extends State<_Demo> {
  final _codeMirror = FlarkCodeMirror();
  FlarkTreeSitter? _treeSitter;
  var _engine = _Engine.codeMirror;
  late FlarkController _controller = _open(_sample, 0);
  String _status = '';
  final _timing = ValueNotifier<String>('');
  double _highlightMs = 0, _editMs = 0;
  bool _reportPending = false;

  FlarkController _open(String text, int caret) {
    final delegate = _Timed(
      _engine == _Engine.codeMirror ? _codeMirror : _treeSitter!,
      (kind, us) {
        // Cache hits cost about a microsecond; report the work.
        if (kind == 'highlight' && us < 20) return;
        if (kind == 'highlight') _highlightMs = us / 1000;
        if (kind == 'edit') _editMs = us / 1000;
        // Highlighting runs during layout: update the bar after the frame.
        if (_reportPending) return;
        _reportPending = true;
        WidgetsBinding.instance.addPostFrameCallback((_) {
          _reportPending = false;
          _timing.value = _engine == _Engine.codeMirror
              ? 'fence highlight ${_highlightMs.toStringAsFixed(2)} ms · '
                    'Enter/typing proposal ${_editMs.toStringAsFixed(2)} ms'
              : 'colors on a worker · Enter/typing proposal '
                    '${_editMs.toStringAsFixed(2)} ms';
        });
        WidgetsBinding.instance.scheduleFrame();
      },
    );
    final editor = FlarkEditor(
      widget.backend,
      codeEditing: delegate,
      text: text,
      caret: caret,
      syncLimit: candidateLiveBytes,
      liveLimits: candidateLiveLimits,
    );
    return FlarkController(
      editor,
      codeColors: _engine == _Engine.treeSitter
          ? FlarkCodeColors(editor)
          : null,
    );
  }

  Future<void> _switch(_Engine engine) async {
    if (engine == _engine) return;
    if (engine == _Engine.treeSitter && _treeSitter == null) {
      setState(() => _status = 'Loading Tree-sitter (13 MB of Wasm)…');
      _treeSitter = await FlarkTreeSitter.load();
    }
    _replace(_controller.text, _controller.editor.selection.extent, engine);
  }

  void _replace(String text, int caret, [_Engine? engine]) {
    final old = _controller;
    setState(() {
      _engine = engine ?? _engine;
      _highlightMs = _editMs = 0;
      _timing.value = '';
      _status = '';
      _controller = _open(text, caret.clamp(0, text.length));
    });
    WidgetsBinding.instance.addPostFrameCallback((_) => old.dispose());
  }

  @override
  void dispose() {
    _controller.dispose();
    _treeSitter?.dispose();
    _timing.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    title: 'CodeMirror engine demo',
    theme: ThemeData(
      colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xff3a628f)),
      useMaterial3: true,
    ),
    home: Scaffold(
      appBar: AppBar(
        title: const Text('Code engine demo: TypeScript'),
        actions: [
          TextButton(
            onPressed: () => _replace(_sample, 0),
            child: const Text('Sample'),
          ),
          TextButton(
            onPressed: () => _replace(_large, 0),
            child: const Text('8K fence'),
          ),
          const SizedBox(width: 8),
          SegmentedButton<_Engine>(
            segments: const [
              ButtonSegment(
                value: _Engine.codeMirror,
                label: Text('CodeMirror'),
              ),
              ButtonSegment(
                value: _Engine.treeSitter,
                label: Text('Tree-sitter'),
              ),
            ],
            selected: {_engine},
            onSelectionChanged: (s) => _switch(s.single),
          ),
          const SizedBox(width: 16),
        ],
      ),
      body: Column(
        children: [
          Container(
            width: double.infinity,
            color: Theme.of(context).colorScheme.surfaceContainerHighest,
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
            child: ValueListenableBuilder(
              valueListenable: _timing,
              builder: (context, timing, _) => Text(
                _status.isNotEmpty
                    ? _status
                    : '${_engine == _Engine.codeMirror ? 'CodeMirror' : 'Tree-sitter'}'
                          '${timing.isEmpty ? '' : ' · $timing'}',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ),
          ),
          Expanded(
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 900),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 24),
                  child: FlarkEditorWidget(
                    key: ObjectKey(_controller),
                    controller: _controller,
                    autofocus: true,
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    ),
  );
}

/// Reports how long each synchronous call into the engine took.
final class _Timed implements CodeEditingDelegate {
  _Timed(this.inner, this.onTiming);
  final CodeEditingDelegate inner;
  final void Function(String kind, int microseconds) onTiming;

  @override
  String resolveLanguage(String source, String info) =>
      inner.resolveLanguage(source, info);

  @override
  CodeHighlight highlight(String source, String info) {
    final watch = Stopwatch()..start();
    final result = inner.highlight(source, info);
    onTiming('highlight', watch.elapsedMicroseconds);
    return result;
  }

  @override
  CodeEditProposal? propose(
    String source, {
    required String language,
    required int base,
    required int extent,
    required CodeEditingAction action,
    required String text,
    required String indentUnit,
  }) {
    final watch = Stopwatch()..start();
    final result = inner.propose(
      source,
      language: language,
      base: base,
      extent: extent,
      action: action,
      text: text,
      indentUnit: indentUnit,
    );
    onTiming('edit', watch.elapsedMicroseconds);
    return result;
  }
}

const _module = r'''import { EventEmitter } from 'node:events';

export interface Task<T = unknown> {
  readonly id: string;
  payload: T;
  priority?: number;
}

type Handler<T> = (task: Task<T>) => Promise<void> | void;

export enum State { Idle, Running = 'running', Done }

export class Queue<T> extends EventEmitter {
  #tasks: Task<T>[] = [];
  private state: State = State.Idle;

  constructor(private readonly limit = 10) {
    super();
  }

  add(task: Task<T>): this {
    if (this.#tasks.length >= this.limit) {
      throw new RangeError(`queue is full: ${this.limit}`);
    }
    this.#tasks.push(task);
    return this;
  }

  async run(handler: Handler<T>): Promise<number> {
    this.state = State.Running;
    let done = 0;
    const ordered = [...this.#tasks].sort(
      (a, b) => (b.priority ?? 0) - (a.priority ?? 0),
    );
    for (const task of ordered) {
      await handler(task);
      done++;
    }
    this.state = State.Done;
    return done;
  }
}''';

final _sample =
    '''# The CodeMirror engine on TypeScript

Fences below are colored and indented by `flark_codemirror`: CodeMirror 5's
JavaScript mode, ported to Dart. Things to try:

- Enter after `{`, and between `{}` or `[]`
- Enter inside an open call, such as after `sort(`
- `}` or `]` typed at the start of an indented line
- Tab and Shift-Tab over selected lines
- The switch at the top: the same document with Tree-sitter

```ts
$_module
```

JavaScript and JSON use the same mode:

```js
const queue = new Queue(2).add({ id: 'a', payload: 1 });
queue.run(async (task) => console.log(task.id));
```

```json
{ "compilerOptions": { "strict": true, "target": "ES2022" }, "include": ["src"] }
```

A fence without a language is detected among these three; other languages
are plain for now.
''';

/// About 8,000 units of TypeScript, just under the engine's snippet cap.
final _large = () {
  final buffer = StringBuffer('# An 8K TypeScript fence\n\n```ts\n');
  for (var i = 0; buffer.length < 7600; i++) {
    buffer
      ..write(_module.replaceAll('Queue', 'Queue$i'))
      ..write('\n\n');
  }
  return '${buffer.toString().trimRight()}\n```\n';
}();
