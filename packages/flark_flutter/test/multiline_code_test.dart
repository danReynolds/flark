import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'multiline inline code paints its space and accepts the next key',
    (tester) async {
      const source = '`a\nb`c`';
      final c = FlarkController(
        FlarkEditor(createParseBackend(), text: source, caret: source.length),
      );
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
      for (final p in paints) {
        expect(p.rows, ['a bc`']);
      }
      paints.clear();
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '${source}x',
          selection: TextSelection.collapsed(offset: source.length + 1),
        ),
      );
      await tester.pump();
      expect(paints, isNotEmpty);
      for (final p in paints) {
        expect(p.rows, ['a bc`x']);
        expect(p.styles.single.take(3), [Style.code, Style.code, Style.code]);
        expect(p.caretSource, source.length + 1);
        expect(p.caret, isNotNull);
      }
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
}
