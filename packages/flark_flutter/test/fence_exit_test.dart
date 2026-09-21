import 'package:flark_flutter/code.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final code = FlarkTreeSitter.fromAnalyzer(CodeAnalyzer());
  tearDownAll(code.dispose);
  for (final width in [240.0, 800.0]) {
    for (final platformValue in [false, true]) {
      testWidgets(
        'double Enter exits and paints at $width, platform=$platformValue',
        (tester) async {
          const source = '```ruby\ndef hello\n```\n\nafter';
          const exited = '```ruby\ndef hello\n```\n\n\nafter';
          final c = FlarkController(
            FlarkEditor(
              createParseBackend(),
              text: source,
              caret: source.indexOf('hello') + 5,
              codeEditing: code,
            ),
          );
          final paints = <FlarkPaintObservation>[];
          await tester.pumpWidget(
            MaterialApp(
              home: Scaffold(
                body: SizedBox(
                  width: width,
                  child: FlarkEditorWidget(
                    controller: c,
                    autofocus: true,
                    onPaint: paints.add,
                  ),
                ),
              ),
            ),
          );
          await tester.pump();

          TextEditingValue type(String text) {
            final sel = c.editor.selection;
            final value = TextEditingValue(
              text: c.text.replaceRange(sel.start, sel.end, text),
              selection: TextSelection.collapsed(
                offset: sel.start + text.length,
              ),
            );
            tester.testTextInput.updateEditingValue(value);
            return value;
          }

          Future<TextEditingValue?> enter() async {
            TextEditingValue? delivered;
            if (platformValue) {
              delivered = type('\n');
            } else {
              await tester.sendKeyEvent(LogicalKeyboardKey.enter);
            }
            await tester.pump();
            return delivered;
          }

          await enter();
          expect(c.editor.document.caretRow.text, 'def hello\n  ');
          expect(find.byTooltip('Code language'), findsOneWidget);
          final inside = c.editor.snapshot;
          paints.clear();
          final delivered = await enter();
          expect(c.text, exited);
          expect(c.editor.document.caretRow.kind, RowKind.blank);
          expect(find.byTooltip('Code language'), findsNothing);
          expect(paints, isNotEmpty);
          for (final paint in paints) {
            expect(paint.snapshot.source, exited);
            expect(paint.rows.first, 'def hello');
            expect(paint.caretSource, c.editor.selection.extent);
            expect(paint.caret, isNotNull);
          }
          final outside = c.editor.selection;
          if (delivered != null) {
            tester.testTextInput.updateEditingValue(delivered);
            await tester.pump();
            expect(
              c.text,
              exited,
            ); // Duplicate callback cannot reinsert the line.
            expect(c.editor.selection, outside);
          }
          c.command(const Undo());
          await tester.pump();
          expect(c.text, inside.source);
          expect(c.editor.selection, inside.selection);
          expect(find.byTooltip('Code language'), findsOneWidget);
          // Shift+Enter deliberately keeps an empty line inside the snippet.
          await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
          await tester.sendKeyEvent(LogicalKeyboardKey.enter);
          await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
          await tester.pump();
          expect(c.editor.document.caretRow.text, 'def hello\n  \n  ');
          c.command(const Undo());
          await tester.pump();
          await enter();
          type('Outside prose');
          await tester.pump();
          expect(c.editor.document.caretRow.kind, RowKind.paragraph);
          expect(c.editor.document.caretRow.text, 'Outside prose');
          expect(c.text, '```ruby\ndef hello\n```\nOutside prose\n\nafter');
          expect(tester.takeException(), isNull);
          await tester.pumpWidget(const SizedBox());
          c.dispose();
        },
      );
    }
  }
}
