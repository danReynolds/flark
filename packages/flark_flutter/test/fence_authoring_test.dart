import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  for (final width in [240.0, 800.0]) {
    testWidgets('typed fence lifecycle paints atomically at width $width', (
      tester,
    ) async {
      const tail = '\n# Heading\n\n```dart\nold\n```';
      final c = FlarkController(FlarkEditor(backend, text: tail));
      final paints = <FlarkPaintObservation>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: width,
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

      Future<void> painted(String source, int caret) async {
        await tester.pump();
        expect(paints, isNotEmpty);
        for (final paint in paints) {
          expect(paint.snapshot.source, source);
          expect(paint.caretSource, caret);
          expect(paint.caret, isNotNull);
          expect(paint.rows, containsAllInOrder(['Heading', 'old']));
        }
        paints.clear();
      }

      void type(String text) {
        final sel = c.editor.selection;
        tester.testTextInput.updateEditingValue(
          TextEditingValue(
            text: c.text.replaceRange(sel.start, sel.end, text),
            selection: TextSelection.collapsed(offset: sel.start + text.length),
          ),
        );
      }

      paints.clear();
      type('`');
      await painted('`$tail', 1);
      type('`');
      await painted('``$tail', 2);
      type('`');
      const created = '```\n\n```\n$tail';
      await painted(created, 4);
      expect(c.editor.document.rowAt(4).text, '');
      expect(c.editor.document.rowAt(4).kind, RowKind.codeBlock);
      type('x');
      const edited = '```\nx\n```\n$tail';
      await painted(edited, 5);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await painted(edited, 10);
      type('prose');
      const outside = '```\nx\n```\nprose$tail';
      await painted(outside, 15);
      expect(c.editor.document.rowAt(15).kind, RowKind.paragraph);
      c.command(const Undo());
      await painted(edited, 10);
      c.command(const Undo());
      await painted(created, 4);
      c.command(const Undo());
      await painted('``$tail', 2);
      c.command(const Redo());
      await painted(created, 4);
      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await painted('\n$tail', 0);
      type('plain');
      await painted('plain\n$tail', 5);
      expect(c.editor.document.rowAt(5).kind, RowKind.paragraph);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    });
  }
}
