import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('clipboard fence text stays code on the first edited paint', (
    tester,
  ) async {
    const before = '> - ```text\n>   here\n>   ```\n\n# after';
    const after = '> - ````text\n>   ```\n>   inside\n>   ````\n\n# after';
    final at = before.indexOf('here');
    final editor = FlarkEditor(createParseBackend(), text: before, caret: at);
    final controller = FlarkController(editor);
    final paints = <FlarkPaintObservation>[];
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FlarkEditorWidget(
            controller: controller,
            autofocus: true,
            showToolbar: false,
            onPaint: paints.add,
          ),
        ),
      ),
    );
    controller.command(SetSelection(at, at + 4));
    await tester.pump();
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.getData') return {'text': '```\ninside'};
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    paints.clear();

    // Deliver the host's real paste action, then inspect its first produced
    // frame without settling or letting a later frame repair its presentation.
    final context = tester.element(find.byType(FlarkSurface));
    final result = Actions.invoke(
      context,
      const PasteTextIntent(SelectionChangedCause.keyboard),
    );
    if (result is Future) await result;
    expect(editor.source, after);
    await tester.pump();
    expect(paints, isNotEmpty);
    for (final paint in paints) {
      expect(identical(paint.snapshot, editor.snapshot), isTrue);
      expect(paint.rows, ['```\ninside', '', 'after']);
      expect(paint.caretSource, after.indexOf('\n>   ````'));
      expect(paint.caret, isNotNull);
    }
    expect(editor.projection.rows.last.kind, RowKind.heading);
    controller.command(const Undo());
    await tester.pump();
    expect(editor.source, before);
    expect(editor.selection, FlarkSelection(at, at + 4));
    await tester.pumpWidget(const SizedBox());
    controller.dispose();
  });
}
