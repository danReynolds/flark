import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  for (final sourceMode in [false, true]) {
    for (final mac in [false, true]) {
      testWidgets(
        'document edges, selection and next key: source=$sourceMode mac=$mac',
        (tester) async {
          const source = '# First\n\n**last**';
          final e = FlarkEditor(backend, text: source, caret: 4);
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
          final modifier = mac
              ? LogicalKeyboardKey.metaLeft
              : LogicalKeyboardKey.controlLeft;
          final end = mac
              ? LogicalKeyboardKey.arrowDown
              : LogicalKeyboardKey.end;
          final start = mac
              ? LogicalKeyboardKey.arrowUp
              : LogicalKeyboardKey.home;
          await tester.sendKeyDownEvent(modifier);
          await tester.sendKeyEvent(end);
          await tester.sendKeyUpEvent(modifier);
          await tester.pump();
          expect(e.selection, const FlarkSelection.collapsed(source.length));
          paints.clear();
          tester.testTextInput.updateEditingValue(
            const TextEditingValue(
              text: '$source!',
              selection: TextSelection.collapsed(offset: source.length + 1),
            ),
          );
          await tester.pump();
          expect(c.text, '$source!');
          expect(paints, isNotEmpty);
          for (final paint in paints) {
            expect(paint.caretSource, source.length + 1);
            expect(paint.revision, e.revision);
            expect(
              paint.rows,
              sourceMode
                  ? ['# First', '', '**last**!']
                  : ['First', '', 'last!'],
            );
          }
          await tester.sendKeyDownEvent(modifier);
          await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
          await tester.sendKeyEvent(start);
          await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
          await tester.sendKeyUpEvent(modifier);
          await tester.pump();
          expect(e.selection, const FlarkSelection(source.length + 1, 0));
          expect(paints.last.selectionRects, isNotEmpty);
          tester.testTextInput.updateEditingValue(
            const TextEditingValue(
              text: 'replacement',
              selection: TextSelection.collapsed(offset: 11),
            ),
          );
          await tester.pump();
          expect(c.text, 'replacement');
          c.command(const Undo());
          expect(c.text, '$source!');
          expect(e.selection, const FlarkSelection(source.length + 1, 0));
          await tester.pumpWidget(const SizedBox());
          c.dispose();
        },
      );
    }
  }
}
