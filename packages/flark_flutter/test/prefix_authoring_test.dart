import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  for (final input in [
    '# H',
    '## H',
    '###### H',
    '*w*',
    '**w**',
    '* x',
    '- x',
    '+ x',
  ]) {
    for (final burst in [false, true]) {
      testWidgets('source authoring $input, burst=$burst', (tester) async {
        final c = FlarkController(FlarkEditor(backend));
        final paints = <FlarkPaintObservation>[];
        await tester.pumpWidget(
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
        await tester.pump();
        final contentLeft = paints.last.caret!.left;
        paints.clear();
        late FlarkPaintObservation finalPaint;

        Future<void> painted(String source, String visible) async {
          await tester.pump();
          expect(paints, isNotEmpty);
          for (final paint in paints) {
            expect(paint.snapshot.source, source);
            expect(paint.caretSource, source.length);
            expect(paint.caret, isNotNull);
            expect(paint.rows.single, visible);
            if (source.endsWith(' ') && input.endsWith('H')) {
              expect(paint.caret!.left, contentLeft);
            }
          }
          finalPaint = paints.last;
          paints.clear();
        }

        void type(String text) {
          final caret = c.editor.selection.extent;
          tester.testTextInput.updateEditingValue(
            TextEditingValue(
              text: c.text.replaceRange(caret, caret, text),
              selection: TextSelection.collapsed(offset: caret + text.length),
            ),
          );
        }

        var source = '';
        for (var i = 0; i < input.length; i++) {
          type(input[i]);
          source += input[i];
          expect(c.text, source);
          expect(c.editor.selection.extent, source.length);
          final complete = i == input.length - 1;
          final visible = complete
              ? input.endsWith('H')
                    ? 'H'
                    : input.endsWith('x')
                    ? 'x'
                    : 'w'
              : input == '**w**' && source == '**w*'
              ? '*w'
              : source.endsWith(' ')
              ? ''
              : source;
          expect(c.editor.projection.rows.single.text, visible);
          if (!burst) await painted(source, visible);
        }
        if (burst) {
          await painted(
            input,
            input.endsWith('H')
                ? 'H'
                : input.endsWith('x')
                ? 'x'
                : 'w',
          );
        }
        if (input.endsWith('H')) {
          expect(
            finalPaint.resolvedStyles.single.single.fontWeight,
            FontWeight.w700,
          );
        } else if (input == '**w**') {
          expect(finalPaint.styles.single, [Style.strong]);
        } else if (input == '*w*') {
          expect(finalPaint.styles.single, [Style.emphasis]);
        }
        paints.clear();
        type('!');
        await painted(
          '$input!',
          input.endsWith('H')
              ? 'H!'
              : input.endsWith('x')
              ? 'x!'
              : 'w!',
        );
        await tester.pumpWidget(const SizedBox());
        c.dispose();
      });
    }
  }
}
