@TestOn('browser')
library;

import 'dart:ui_web' as ui_web;
import 'dart:js_interop';
import 'package:flark/wasm.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:web/web.dart' as web;

void main() {
  // Ahem makes every glyph equal-width and would conceal this regression.
  ui_web.TestEnvironment.setUp(
    const ui_web.TestEnvironment(
      forceTestFonts: false,
      disableFontFallbacks: false,
      keepSemanticsDisabledOnUpdate: true,
      defaultToTestUrlStrategy: true,
    ),
  );
  final binding = WidgetsFlutterBinding.ensureInitialized();
  test(
    'actual code and source caret columns use a loaded monospace face',
    () async {
      final response = await web.window
          .fetch(
            '/packages/flark_flutter/assets/fonts/RobotoMono-Regular.otf'.toJS,
          )
          .toDart;
      expect(response.ok, isTrue);
      final bytes = (await response.arrayBuffer().toDart).toDart;
      final loader = FontLoader('packages/flark_flutter/FlarkMono')
        ..addFont(Future.value(ByteData.view(bytes)));
      await loader.load();
      final backend = await WasmParseBackend.load(
        candidates: [
          Uri.base.resolve('/packages/flark/assets/wasm/flark_parse.wasm'),
        ],
      );
      // Negative control: an unregistered family falls back to proportional text.
      double width(String text) {
        final p = TextPainter(
          text: TextSpan(
            text: text,
            style: const TextStyle(
              fontFamily: 'MissingFlarkTestFace',
              fontSize: 15,
            ),
          ),
          textDirection: TextDirection.ltr,
        )..layout();
        final value = p.width;
        p.dispose();
        return value;
      }

      expect((width('iiiiiiii') - width('WWWWWWWW')).abs(), greaterThan(20));
      const source = '```text\niiiiiiii\nWWWWWWWW\n```';
      final first = source.indexOf('iiiiiiii') + 8,
          second = source.indexOf('WWWWWWWW') + 8;
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: first),
      );
      final paints = <FlarkPaintObservation>[];
      addTearDown(() async {
        runApp(const SizedBox());
        await binding.endOfFrame;
        c.dispose();
      });
      runApp(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              autofocus: true,
              showToolbar: false,
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await binding.endOfFrame;
      for (final sourceMode in [false, true]) {
        c.editor.setSourceMode(sourceMode);
        c.command(SetSelection.caret(first));
        await binding.endOfFrame;
        final x = paints.last.caret!.left;
        c.command(SetSelection.caret(second));
        await binding.endOfFrame;
        expect(
          paints.last.caret!.left,
          closeTo(x, .01),
          reason: 'equal code columns, sourceMode=$sourceMode',
        );
        expect(c.text, source);
      }
    },
  );
}
