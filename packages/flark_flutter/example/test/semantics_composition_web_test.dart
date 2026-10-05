@TestOn('browser')
library;

import 'dart:js_interop';
import 'dart:ui_web' as ui_web;
import 'package:flark/wasm.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:web/web.dart' as web;

import 'web_input_test.dart' show compose, composition;

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
    'accessibility turned on after the editor took input keeps a composition',
    () async {
      // A screen reader turns semantics on after the page has loaded, when the
      // editor already holds the input connection, so the engine keeps its
      // hidden input element. It moved DOM focus to the editor's semantics
      // element whenever that node changed, and a browser ends its
      // composition when the input element loses focus: every composed update
      // was committed and the next appended ("nににほ日本" for 日本). Found by
      // tool/browser_fuzz.mjs; Flutter's own TextField does the same.
      final backend = await WasmParseBackend.load(
        candidates: [
          Uri.base.resolve('/packages/flark/assets/wasm/flark_parse.wasm'),
        ],
      );
      SemanticsHandle? semantics;
      addTearDown(() => semantics?.dispose());
      // Composes [steps] (each the composed text and the field's value) at
      // the start of [text] in an editor [width] wide, and commits the last.
      Future<void> composeIn(
        String text,
        double width,
        List<(String, String)> steps, {
        int caret = 0,
      }) async {
        final c = FlarkController(
          FlarkEditor(backend, text: text, caret: caret),
        );
        final focus = FocusNode();
        runApp(
          MaterialApp(
            home: Scaffold(
              body: Align(
                alignment: Alignment.topLeft,
                child: SizedBox(
                  width: width,
                  height: 200,
                  child: FlarkEditorWidget(
                    controller: c,
                    focusNode: focus,
                    autofocus: true,
                    showToolbar: false,
                  ),
                ),
              ),
            ),
          ),
        );
        await binding.endOfFrame;
        await Future<void>.delayed(Duration.zero);
        focus.requestFocus();
        await binding.endOfFrame;
        final input =
            web.document.querySelector('textarea.flt-text-editing')!
                as web.HTMLTextAreaElement;
        // A screen reader turns accessibility on once the editor has input.
        semantics ??= binding.ensureSemantics();
        for (var i = 0; i < 3; i++) {
          await binding.endOfFrame;
          await Future<void>.delayed(Duration.zero);
        }
        expect(
          web.document.querySelector(
            'textarea[data-semantics-role="text-field"]',
          ),
          isNotNull,
        );
        expect(web.document.activeElement, input);
        var blurs = 0;
        final blurred = ((web.Event _) => blurs++).toJS;
        input.addEventListener('blur', blurred);
        composition(input, 'compositionstart', '');
        final at = input.selectionStart;
        for (final (data, value) in steps) {
          compose(input, data, value, at + data.length);
          await Future<void>.delayed(Duration.zero);
          await binding.endOfFrame;
          await Future<void>.delayed(Duration.zero);
          expect(
            blurs,
            0,
            reason: 'the input element lost focus composing $data',
          );
          expect((c.text, c.editor.composing), (value, true));
        }
        input.removeEventListener('blur', blurred);
        final (committed, value) = steps.last;
        composition(input, 'compositionend', committed);
        web.document.dispatchEvent(web.Event('selectionchange'));
        await Future<void>.delayed(Duration.zero);
        await binding.endOfFrame;
        expect(
          (c.text, c.editor.selection, c.editor.composing),
          (
            value,
            FlarkSelection.collapsed(
              value.indexOf(committed) + committed.length,
            ),
            false,
          ),
        );
        // The semantics describe the committed document as it shows.
        await binding.endOfFrame;
        String? field;
        bool visit(SemanticsNode node) {
          if (node.getSemanticsData().flagsCollection.isTextField) {
            field = node.value;
            return false;
          }
          node.visitChildren(visit);
          return true;
        }

        visit(
          binding.renderViews.first.owner!.semanticsOwner!.rootSemanticsNode!,
        );
        expect(field, c.editor.projection.rows.map((r) => r.text).join('\n'));
        runApp(const SizedBox());
        await binding.endOfFrame;
        c.dispose();
        focus.dispose();
      }

      await composeIn('abc', 600, [
        ('n', 'nabc'),
        ('に', 'にabc'),
        ('にほ', 'にほabc'),
        ('日本', '日本abc'),
      ]);
      // Composed text that wraps makes a document taller than the editor
      // grow, and the editor's semantics node with it.
      final lines = [for (var i = 0; i < 12; i++) 'line $i'].join('\n\n');
      await composeIn(lines, 160, [
        for (var n = 1; n <= 22; n += 3) ('あ' * n, '$lines${'あ' * n}'),
      ], caret: lines.length);
    },
  );
}
