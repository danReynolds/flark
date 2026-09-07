import 'dart:ui' as ui;
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'a late font repaints cached editor glyphs without a source edit',
    (tester) async {
      final c = FlarkController(
        FlarkEditor(
          createParseBackend(),
          text: String.fromCharCode(Icons.star.codePoint),
        ),
      );
      final boundary = GlobalKey();
      await tester.pumpWidget(
        MaterialApp(
          home: RepaintBoundary(
            key: boundary,
            child: ColoredBox(
              color: Colors.white,
              child: FlarkEditorWidget(
                controller: c,
                showToolbar: false,
                style: FlarkEditorWidget.defaultStyle.copyWith(
                  fontFamily: 'LateFlarkTestFont',
                ),
              ),
            ),
          ),
        ),
      );
      Future<List<int>> pixels() async {
        final render =
            boundary.currentContext!.findRenderObject()!
                as RenderRepaintBoundary;
        final image = await render.toImage();
        final data = (await image.toByteData(
          format: ui.ImageByteFormat.rawRgba,
        ))!.buffer.asUint8List();
        final result = List<int>.of(data);
        image.dispose();
        return result;
      }

      final before = (await tester.runAsync(pixels))!;
      final snapshot = c.editor.snapshot;
      final loader = FontLoader('LateFlarkTestFont')
        ..addFont(rootBundle.load('fonts/MaterialIcons-Regular.otf'));
      await tester.runAsync(loader.load);
      await tester.pump();
      final after = (await tester.runAsync(pixels))!;
      expect(c.editor.snapshot, same(snapshot));
      expect(
        after,
        isNot(equals(before)),
        reason: 'a loaded font must replace cached fallback glyph pixels',
      );
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
}
