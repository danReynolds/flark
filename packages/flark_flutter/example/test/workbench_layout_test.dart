import 'package:flark_dogfood/main.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  testWidgets('resize and inspection preserve the current editable document', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({});
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.view.resetPhysicalSize);
    final paints = <FlarkPaintObservation>[];
    await tester.pumpWidget(
      DogfoodApp(
        backend: createParseBackend(),
        preferences: await SharedPreferences.getInstance(),
        onPaint: paints.add,
      ),
    );
    await tester.pump();
    final c = tester
        .widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget))
        .controller;
    c.command(SetSelection.caret(c.text.length));
    expect(c.command(const InsertText('end')), isTrue);
    await tester.pump();
    final source = c.text, selection = c.editor.selection;
    await tester.tap(find.byTooltip('Inspect Markdown'));
    for (final width in [320.0, 500.0, 1000.0]) {
      paints.clear();
      tester.view.physicalSize = Size(width, 600);
      await tester.pump();
      expect(tester.takeException(), isNull);
      expect(c.text, source);
      expect(c.editor.selection, selection);
      expect(paints, isNotEmpty);
      expect(paints.last.caret, isNotNull);
      expect(
        find.byType(FlarkSourceView),
        width > 650 ? findsOneWidget : findsNothing,
      );
    }
    await tester.pumpWidget(const SizedBox());
  });
}
