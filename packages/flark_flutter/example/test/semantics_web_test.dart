@TestOn('browser')
library;

import 'dart:ui_web' as ui_web;
import 'package:flark/wasm.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:web/web.dart' as web;

void main() {
  ui_web.TestEnvironment.setUp(
    const ui_web.TestEnvironment(
      forceTestFonts: true,
      disableFontFallbacks: true,
      keepSemanticsDisabledOnUpdate: false,
      defaultToTestUrlStrategy: true,
    ),
  );
  final binding = WidgetsFlutterBinding.ensureInitialized();
  test(
    'enabled semantics field can focus, edit styled text and undo',
    () async {
      final semantics = binding.ensureSemantics();
      final backend = await WasmParseBackend.load(
        candidates: [
          Uri.base.resolve('/packages/flark/assets/wasm/flark_parse.wasm'),
        ],
      );
      final c = FlarkController(
        FlarkEditor(backend, text: '**ab** cd', caret: 4),
      );
      final focus = FocusNode();
      final paints = <FlarkPaintObservation>[];
      addTearDown(() async {
        runApp(const SizedBox());
        await binding.endOfFrame;
        c.dispose();
        focus.dispose();
        semantics.dispose();
      });
      runApp(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              focusNode: focus,
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await binding.endOfFrame;
      await Future<void>.delayed(Duration.zero);
      // Let the initial route's semantics focus settle before user focus.
      await binding.endOfFrame;
      await Future<void>.delayed(Duration.zero);
      final input =
          web.document.querySelector(
                'textarea[data-semantics-role="text-field"]',
              )!
              as web.HTMLTextAreaElement;
      expect(
        input.disabled,
        isFalse,
        reason: 'A declared editor must be enabled.',
      );
      input.focus();
      await Future<void>.delayed(Duration.zero);
      await binding.endOfFrame;
      await Future<void>.delayed(Duration.zero);
      expect(focus.hasFocus, isTrue);
      expect(web.document.activeElement, input);
      expect(input.value, '**ab** cd');
      expect((input.selectionStart, input.selectionEnd), (4, 4));
      for (final character in ['x', 'y']) {
        final at = input.selectionStart;
        input.value = input.value.replaceRange(
          at,
          input.selectionEnd,
          character,
        );
        input.setSelectionRange(at + 1, at + 1);
        paints.clear();
        input.dispatchEvent(
          web.InputEvent('input', web.InputEventInit(bubbles: true)),
        );
        await Future<void>.delayed(Duration.zero);
        await binding.endOfFrame;
        final word = character == 'x' ? 'abx' : 'abxy';
        expect(c.text, '**$word** cd');
        expect(paints, isNotEmpty);
        for (final paint in paints) {
          expect(paint.rows, ['$word cd']);
          expect(paint.styles.single, contains(Style.strong));
          expect(paint.caretSource, word.length + 2);
        }
      }
      c.command(const Undo());
      await binding.endOfFrame;
      expect(c.text, '**ab** cd');
      expect(input.value, '**ab** cd');
    },
  );
}
