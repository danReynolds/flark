import 'dart:ui' as ui;
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('Select All scopes to code, then expands to the document', (
    tester,
  ) async {
    const source = 'Before.\n\n```ruby\ndef test\n\nend\n```\n\nAfter.';
    final start = source.indexOf('def test'),
        end = source.indexOf('\n```\n\nAfter');
    final c = FlarkController(
      FlarkEditor(createParseBackend(), text: source, caret: start + 4),
    );
    final paints = <FlarkPaintObservation>[];
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FlarkEditorWidget(
            controller: c,
            autofocus: true,
            onPaint: paints.add,
          ),
        ),
      ),
    );
    await tester.pump();
    Future<void> selectAll() async {
      await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyA);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
      await tester.pump();
    }

    await selectAll();
    expect(c.editor.selection, FlarkSelection(start, end));
    expect(c.selectedText, 'def test\n\nend');
    expect(paints.last.selectionRects, isNotEmpty);
    expect(c.notice, isNull);
    await selectAll();
    expect(c.editor.selection, const FlarkSelection(0, source.length));
    await selectAll();
    expect(c.editor.selection, const FlarkSelection(0, source.length));
    expect(c.notice, isNull);
    c.command(SetSelection.caret(start + 4));
    await selectAll();
    c.command(const Paste('puts "hello"\nputs "again"'));
    c.command(const InsertText('!'));
    await tester.pump();
    expect(
      c.text,
      'Before.\n\n```ruby\nputs "hello"\nputs "again"!\n```\n\nAfter.',
    );
    expect(paints.last.rows, contains('puts "hello"\nputs "again"!'));
    c.command(const Undo());
    c.command(const Undo());
    expect(c.text, source);
    expect(c.editor.selection, FlarkSelection(start, end));
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
  testWidgets(
    'dragged code selection is visibly blue and replacement is undoable',
    (tester) async {
      const source = '```\nalpha    omega\nsecond\n```';
      final c = FlarkController(
        FlarkEditor(createParseBackend(), text: source, caret: 9),
      );
      final paints = <FlarkPaintObservation>[];
      final raster = GlobalKey();
      await tester.pumpWidget(
        MaterialApp(
          theme: ThemeData(
            textSelectionTheme: const TextSelectionThemeData(
              selectionColor: Color(0x553c82ed),
            ),
          ),
          home: Scaffold(
            body: RepaintBoundary(
              key: raster,
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
      await tester.pump();
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkSurface),
      );
      final from = surface.localToGlobal(surface.caretRect.center);
      c.command(const SetSelection.caret(13));
      await tester.pump();
      final to = surface.localToGlobal(surface.caretRect.center);
      final mouse = await tester.startGesture(
        from,
        kind: PointerDeviceKind.mouse,
      );
      await mouse.moveTo(to);
      await mouse.up();
      paints.clear();
      await tester.pump();
      expect(c.editor.selection, const FlarkSelection(9, 13));
      expect(paints, isNotEmpty);
      expect(paints.last.selectionRects, isNotEmpty);
      final blue = await tester.runAsync(() async {
        final boundary =
            raster.currentContext!.findRenderObject() as RenderRepaintBoundary;
        final image = await boundary.toImage();
        final data = (await image.toByteData(
          format: ui.ImageByteFormat.rawRgba,
        ))!;
        final point = paints.last.selectionRects.single.center;
        final i = (point.dy.floor() * image.width + point.dx.floor()) * 4;
        final visible =
            data.getUint8(i + 2) > data.getUint8(i) + 30 &&
            data.getUint8(i + 2) > data.getUint8(i + 1) + 10;
        image.dispose();
        return visible;
      });
      expect(blue, isTrue);
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '```\nalphanewomega\nsecond\n```',
          selection: TextSelection.collapsed(offset: 12),
        ),
      );
      await tester.pump();
      expect(c.text, '```\nalphanewomega\nsecond\n```');
      c.command(const Undo());
      expect(c.text, source);
      expect(c.editor.selection, const FlarkSelection(9, 13));
      await tester.pumpWidget(const SizedBox());
      await tester.pump(kDoubleTapMinTime);
      c.dispose();
    },
  );
}
