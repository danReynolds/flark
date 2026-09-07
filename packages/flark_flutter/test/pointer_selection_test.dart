import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  for (final source in [
    'A simple **bold** word.',
    'A simple [bold](https://example.com) word.',
    'A simple `bold` word.',
  ]) {
    testWidgets('double click selects the visible word: $source', (
      tester,
    ) async {
      final start = source.indexOf('bold');
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: start + 2),
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
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkSurface),
      );
      final point = surface.localToGlobal(surface.caretRect.center);
      await tester.tapAt(point, kind: PointerDeviceKind.mouse);
      await tester.pump(kDoubleTapMinTime);
      paints.clear();
      await tester.tapAt(point, kind: PointerDeviceKind.mouse);
      await tester.pump();
      expect(c.editor.selection, FlarkSelection(start, start + 4));
      expect(paints, isNotEmpty);
      expect(paints.last.selectionRects, isNotEmpty);
      paints.clear();
      tester.testTextInput.updateEditingValue(
        TextEditingValue(
          text: source.replaceRange(start, start + 4, 'new'),
          selection: TextSelection.collapsed(offset: start + 3),
        ),
      );
      await tester.pump();
      expect(c.text, source.replaceRange(start, start + 4, 'new'));
      expect(paints, isNotEmpty);
      for (final paint in paints) {
        expect(paint.rows, ['A simple new word.']);
        expect(paint.caretSource, start + 3);
        expect(paint.revision, c.editor.revision);
      }
      c.command(const Undo());
      await tester.pump();
      expect(c.text, source);
      expect(c.editor.selection, FlarkSelection(start, start + 4));
      await tester.pumpWidget(const SizedBox());
      // The engine's double-tap minimum-interval timer outlives its tracker.
      // Flush it only after the proving paints and unmount assertions.
      await tester.pump(kDoubleTapMinTime);
      c.dispose();
    });
  }
}
