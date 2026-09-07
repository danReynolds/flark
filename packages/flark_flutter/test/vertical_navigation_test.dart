import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  testWidgets(
    'vertical movement traverses blank code lines and block boundaries',
    (tester) async {
      const source = 'top\n\n```\n\nx\n\n```\n\nend';
      final c = FlarkController(FlarkEditor(backend, text: source, caret: 0));
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
      var top = paints.last.caret!.top;
      for (final target in [4, 9, 10, 12, 17, 18]) {
        paints.clear();
        await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
        expect(c.editor.selection.extent, target);
        await tester.pump();
        expect(paints, isNotEmpty);
        expect(paints.first.caret!.top, greaterThan(top));
        top = paints.last.caret!.top;
      }
      for (final target in [17, 12, 10, 9, 4, 0]) {
        paints.clear();
        await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
        expect(c.editor.selection.extent, target);
        await tester.pump();
        expect(paints, isNotEmpty);
        expect(paints.first.caret!.top, lessThan(top));
        top = paints.last.caret!.top;
      }
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
  for (final sourceMode in [false, true]) {
    testWidgets('vertical arrows cross every blank line: source=$sourceMode', (
      tester,
    ) async {
      const source = 'before\n\n\n\nafter';
      final e = FlarkEditor(backend, text: source, caret: source.length);
      if (sourceMode) e.setSourceMode(true);
      final c = FlarkController(e);
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
      var top = paints.last.caret!.top;
      for (final target in [9, 8, 7, 5]) {
        paints.clear();
        await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
        expect(e.selection.extent, target);
        await tester.pump();
        expect(paints, isNotEmpty);
        for (final paint in paints) {
          expect(paint.caretSource, target);
          expect(paint.caret!.top, lessThan(top));
        }
        top = paints.last.caret!.top;
      }
      for (final target in [7, 8, 9, 15]) {
        paints.clear();
        await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
        expect(e.selection.extent, target);
        await tester.pump();
        expect(paints, isNotEmpty);
        expect(paints.first.caret!.top, greaterThan(top));
        top = paints.last.caret!.top;
      }
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      for (var i = 0; i < 3; i++) {
        await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
      }
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      expect(e.selection, const FlarkSelection(15, 7));
      await tester.pump();
      expect(paints.last.selectionRects, isNotEmpty);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: 'before\nx\n\n\nafter',
          selection: TextSelection.collapsed(offset: 8),
        ),
      );
      await tester.pump();
      expect(c.text, 'before\nx\n\n\nafter');
      expect(e.selection.extent, 8);
      c.command(const Undo());
      expect(c.text, source);
      expect(e.selection.extent, 7);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    });
  }
}
