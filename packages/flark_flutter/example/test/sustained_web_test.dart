@TestOn('browser')
library;

import 'dart:convert';
import 'dart:ui_web' as ui_web;
import 'package:flark/wasm.dart';
import 'package:flark_flutter/code.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/wasm.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:web/web.dart' as web;

// This exercises real browser input and coloring workers over sustained edits.
// Debug-browser paint latency is diagnostic; it is not a release raster budget.
void main() {
  ui_web.TestEnvironment.setUp(
    const ui_web.TestEnvironment(
      forceTestFonts: true,
      disableFontFallbacks: true,
      keepSemanticsDisabledOnUpdate: true,
      defaultToTestUrlStrategy: true,
    ),
  );
  final binding = WidgetsFlutterBinding.ensureInitialized();
  late FlarkParseBackend backend;
  late FlarkTreeSitter code;
  setUpAll(() async {
    backend = await WasmParseBackend.load(
      candidates: [
        Uri.base.resolve('/packages/flark/assets/wasm/flark_parse.wasm'),
      ],
    );
    code = FlarkTreeSitter.fromAnalyzer(
      CodeAnalyzer(
        backend: await WasmCodeBackend.load(
          uri: Uri.base.resolve(
            '/packages/flark_tree_sitter/assets/wasm/flark_tree_sitter.wasm',
          ),
        ),
      ),
    );
  });
  tearDownAll(() => code.dispose());

  for (final (label, info, initialBody) in [
    (
      'Automatic Ruby',
      '',
      'def hello\n  message = ""\nend\n# ${'context ' * 60}',
    ),
    ('large Dart', 'dart', 'final message = "";\n// ${'padding ' * 350}'),
  ]) {
    test(
      'sustained browser input and color transitions: $label',
      () async {
        String wrap(String body) =>
            '```$info\n$body\n```\n\n# Following heading';
        final original = wrap(initialBody);
        final at = original.indexOf('""') + 1;
        final editor = FlarkEditor(
          backend,
          text: original,
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
        final focus = FocusNode();
        final paints = <FlarkPaintObservation>[];
        addTearDown(() async {
          runApp(const SizedBox());
          await binding.endOfFrame;
          c.dispose();
          focus.dispose();
        });
        runApp(
          MaterialApp(
            home: Scaffold(
              body: FlarkEditorWidget(
                controller: c,
                focusNode: focus,
                autofocus: true,
                onPaint: paints.add,
              ),
            ),
          ),
        );
        await binding.endOfFrame;
        await Future<void>.delayed(Duration.zero);
        focus.requestFocus();
        await binding.endOfFrame;

        Future<void> requireCurrentColors() async {
          final stop = Stopwatch()..start();
          final row = editor.document.rowAt(editor.selection.extent);
          while (!colors
                  .highlight(row.text, info)
                  .tokens
                  .any((t) => t.kind != null) &&
              colors.failure == null &&
              stop.elapsedMilliseconds < 15000) {
            await Future<void>.delayed(const Duration(milliseconds: 5));
          }
          expect(colors.failure, isNull);
          expect(
            colors.highlight(row.text, info).tokens.any((t) => t.kind != null),
            isTrue,
          );
          await binding.endOfFrame;
          expect(
            paints.last.resolvedStyles.first.map((s) => s.color).toSet().length,
            greaterThan(1),
          );
        }

        await requireCurrentColors();
        var expected = original;
        var caret = at;
        final contexts = <web.HTMLTextAreaElement>{};
        final callbackUs = <int>[], firstPaintUs = <int>[];
        final watch = Stopwatch()..start();
        final started = watch.elapsedMicroseconds;
        const text = 'alpha beta  gamma!';
        for (var tick = 0; tick < 1500; tick++) {
          final insert = tick < 750;
          final character = text[tick % text.length];
          expected = insert
              ? expected.replaceRange(caret, caret, character)
              : expected.replaceRange(caret - 1, caret, '');
          caret += insert ? 1 : -1;
          final input =
              web.document.querySelector('textarea.flt-text-editing')!
                  as web.HTMLTextAreaElement;
          contexts.add(input);
          final local = input.selectionStart;
          final end = input.selectionEnd;
          expect(local, end);
          final start = watch.elapsedMicroseconds;
          input.dispatchEvent(
            web.InputEvent(
              'beforeinput',
              web.InputEventInit(
                bubbles: true,
                cancelable: true,
                inputType: insert ? 'insertText' : 'deleteContentBackward',
                data: insert ? character : null,
              ),
            ),
          );
          input.value = insert
              ? input.value.replaceRange(local, end, character)
              : input.value.replaceRange(local - 1, end, '');
          final next = local + (insert ? 1 : -1);
          input.setSelectionRange(next, next);
          paints.clear();
          input.dispatchEvent(
            web.InputEvent(
              'input',
              web.InputEventInit(
                bubbles: true,
                inputType: insert ? 'insertText' : 'deleteContentBackward',
                data: insert ? character : null,
              ),
            ),
          );
          callbackUs.add(watch.elapsedMicroseconds - start);
          await Future<void>.delayed(Duration.zero);
          await binding.endOfFrame;
          firstPaintUs.add(watch.elapsedMicroseconds - start);
          expect(c.text, expected, reason: '$label input $tick');
          expect(editor.selection.extent, caret);
          expect(editor.sourceMode, isFalse);
          expect(editor.projection.rows.last.kind, RowKind.heading);
          expect(editor.projection.rows.last.text, 'Following heading');
          expect(paints, isNotEmpty);
          for (final paint in paints) {
            expect(paint.snapshot, same(editor.snapshot));
            expect(paint.snapshot.source, expected);
            expect(paint.caretSource, caret);
            expect(paint.caret, isNotNull);
          }
          expect(colors.failure, isNull);
          if ((tick + 1) % 250 == 0) {
            await requireCurrentColors();
            debugPrint('FLARK_BROWSER_SUSTAINED $label ${tick + 1}/1500');
          }
          if (tick == 749) {
            expect(c.command(const Undo()), isTrue);
            await binding.endOfFrame;
            expect(c.text, original);
            expect(c.command(const Redo()), isTrue);
            await binding.endOfFrame;
            expect(c.text, expected);
            expect(editor.selection.extent, caret);
          }
          await Future<void>.delayed(const Duration(milliseconds: 100));
        }
        expect(expected, original);
        expect(c.text, original);
        expect(
          contexts.length,
          greaterThan(1),
          reason: 'real input contexts must rebase',
        );
        expect(c.command(const Undo()), isTrue);
        expect(c.command(const Redo()), isTrue);
        await binding.endOfFrame;
        expect(c.text, original);
        expect(editor.selection.extent, at);
        int percentile(List<int> values, double p) {
          final sorted = [...values]..sort();
          return sorted[(sorted.length * p).ceil() - 1];
        }

        debugPrint(
          'FLARK_BROWSER_SUSTAINED_RECEIPT ${jsonEncode({'label': label, 'inputs': 1500, 'inputContexts': contexts.length, 'elapsedUs': watch.elapsedMicroseconds - started, 'callbackP99Us': percentile(callbackUs, .99), 'firstPaintObservedP99Us': percentile(firstPaintUs, .99), 'colorRevisions': colors.revision, 'restoredExactSource': c.text == original, 'evidence': 'debug browser transport and actual paint; not release raster timing'})}',
        );
      },
      timeout: const Timeout(Duration(minutes: 5)),
    );
  }
}
