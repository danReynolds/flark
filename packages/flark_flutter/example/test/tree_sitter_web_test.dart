@TestOn('browser')
library;

import 'dart:convert';
import 'package:flark/wasm.dart';
import 'package:flark_flutter/code.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/wasm.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final binding = WidgetsFlutterBinding.ensureInitialized();
  late FlarkParseBackend backend;
  late FlarkTreeSitter code;
  setUpAll(() async {
    backend = await WasmParseBackend.load(
      candidates: [
        Uri.base.resolve('/packages/flark/assets/wasm/flark_parse.wasm'),
      ],
    );
    final watch = Stopwatch()..start();
    code = FlarkTreeSitter.fromAnalyzer(
      CodeAnalyzer(
        backend: await WasmCodeBackend.load(
          uri: Uri.base.resolve(
            '/packages/flark_tree_sitter/assets/wasm/flark_tree_sitter.wasm',
          ),
        ),
      ),
    );
    // Receipt is a browser test measurement, not a release/profile frame gate.
    debugPrint(
      'TREE_SITTER_COLD ${jsonEncode({'loadAndWarmUs': watch.elapsedMicroseconds})}',
    );
  });
  tearDownAll(() => code.dispose());

  for (final (language, before, after, typed) in [
    (
      'dart',
      'for (final x in [1, 2]) {\n  ¦',
      'for (final x in [1, 2]) {\n}¦',
      '}',
    ),
    ('javascript', 'const x = {\n  ¦', 'const x = {\n}¦', '}'),
    ('typescript', 'interface User {\n  ¦', 'interface User {\n}¦', '}'),
    ('rust', 'fn main() {¦', 'fn main() {\n  ¦', '\n'),
    ('go', 'func main() {¦', 'func main() {\n  ¦', '\n'),
    ('json', '{¦"answer": 42}', '{\n  ¦"answer": 42}', '\n'),
    ('css', '.card {¦', '.card {\n  ¦', '\n'),
    ('bash', 'if true; then¦', 'if true; then\n  ¦', '\n'),
    ('html', '<div>¦</div>', '<div>\n  ¦\n</div>', '\n'),
    ('xml', '<item>¦</item>', '<item>\n  ¦\n</item>', '\n'),
    ('sql', 'SELECT (¦', 'SELECT (\n  ¦', '\n'),
    ('', 'def hello¦', 'def hello\n  ¦', '\n'),
    (
      '',
      'if true; then\n  echo "hello"\n  f¦',
      'if true; then\n  echo "hello"\nfi¦',
      'i',
    ),

    (
      'python',
      'if ready:\n    run()\n    else¦',
      'if ready:\n    run()\nelse:¦',
      ':',
    ),
    ('yaml', 'settings: |¦', 'settings: |\n  ¦', '\n'),
    ('ruby', 'def hello¦', 'def hello\n  ¦', '\n'),
    (
      'ruby',
      "def hello\n  print 'test'\n  en¦",
      "def hello\n  print 'test'\nend¦",
      'd',
    ),
  ]) {
    test(
      'real Wasm worker and Flutter input/paint: $language ${typed == '\n' ? 'Enter' : 'outdent'}',
      () async {
        String wrap(String s) =>
            '> - ```$language\n>   ${s.replaceAll('\n', '\n>   ')}\n>   ```\n\nafter';
        final initial = wrap(before), expected = wrap(after);
        final source = initial.replaceFirst('¦', ''), at = initial.indexOf('¦');
        final editor = FlarkEditor(
          backend,
          text: source,
          caret: at,
          codeEditing: code,
        );
        final colors = FlarkCodeColors(
          editor,
          workerUri: Uri.base.resolve(
            '/packages/flark_tree_sitter/assets/highlight_worker.mjs',
          ),
          wasmUri: Uri.base.resolve(
            '/packages/flark_tree_sitter/assets/wasm/flark_tree_sitter.wasm',
          ),
        );
        final c = FlarkController(editor, codeColors: colors);
        final paints = <FlarkPaintObservation>[];
        addTearDown(() async {
          runApp(const SizedBox());
          await binding.endOfFrame;
          c.dispose();
        });
        runApp(
          MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: 500,
                child: FlarkEditorWidget(
                  controller: c,
                  autofocus: true,
                  showToolbar: false,
                  onPaint: paints.add,
                ),
              ),
            ),
          ),
        );
        await binding.endOfFrame;

        Future<void> colored(int previous) async {
          final stop = Stopwatch()..start();
          while (colors.revision <= previous &&
              colors.failure == null &&
              stop.elapsedMilliseconds < 15000) {
            await Future<void>.delayed(const Duration(milliseconds: 5));
          }
          expect(colors.failure, isNull);
          expect(colors.revision, greaterThan(previous));
          await binding.endOfFrame;
        }

        await colored(0);
        expect(
          paints.last.resolvedStyles.first.map((s) => s.color).toSet().length,
          greaterThan(1),
        );
        final colorRevision = colors.revision;
        paints.clear();
        final watch = Stopwatch()..start();
        // Deliver a platform value to the same controller path as Flutter input.
        c.receive(
          TextEditingValue(
            text: source.replaceRange(at, at, typed),
            selection: TextSelection.collapsed(offset: at + typed.length),
          ),
        );
        final callbackUs = watch.elapsedMicroseconds;
        final edited = expected.replaceFirst('¦', ''),
            caret = expected.indexOf('¦');
        expect((c.text, editor.selection.extent), (edited, caret));
        final snapshot = editor.snapshot, revision = editor.revision;
        await binding.endOfFrame;
        final firstFrameUs = watch.elapsedMicroseconds;
        expect(paints, isNotEmpty);
        expect(paints.first.caretSource, caret);
        expect(paints.first.rows.first, after.replaceFirst('¦', ''));
        final firstCaret = paints.first.caret;
        await colored(colorRevision);
        expect(editor.snapshot, same(snapshot));
        expect(editor.revision, revision);
        expect(paints.last.caret, firstCaret);
        expect(
          paints.last.resolvedStyles.first.map((s) => s.color).toSet().length,
          greaterThan(1),
        );
        debugPrint(
          'TREE_SITTER_FRAME ${jsonEncode({'language': language, 'callbackUs': callbackUs, 'firstPaintWaitUs': firstFrameUs, 'colorAndPaintWaitUs': watch.elapsedMicroseconds, 'paints': paints.length})}',
        );
        c.receive(
          TextEditingValue(
            text: edited.replaceRange(caret, caret, 'z'),
            selection: TextSelection.collapsed(offset: caret + 1),
          ),
        );
        expect(c.text, edited.replaceRange(caret, caret, 'z'));
        c.command(const Undo());
        expect((c.text, editor.selection.extent), (edited, caret));
        c.command(const Undo());
        expect((c.text, editor.selection.extent), (source, at));
        c.command(const Redo());
        expect((c.text, editor.selection.extent), (edited, caret));
        await binding.endOfFrame;
      },
    );
  }
}
