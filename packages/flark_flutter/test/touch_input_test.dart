import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  final paragraphs = List.generate(
    60,
    (i) => 'Paragraph $i has a few words.',
  ).join('\n\n');

  Future<(FlarkController, RenderFlarkSurface, ScrollPosition)> pumpEditor(
    WidgetTester tester, {
    String? text,
    int caret = 0,
    bool readOnly = false,
    ValueChanged<Uri>? onOpenLink,
  }) async {
    final c = FlarkController(
      FlarkEditor(backend, text: text ?? paragraphs, caret: caret),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            height: 400,
            child: FlarkEditorWidget(
              controller: c,
              readOnly: readOnly,
              showToolbar: false,
              onOpenLink: onOpenLink,
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    final surface = tester.renderObject<RenderFlarkSurface>(
      find.byType(FlarkSurface),
    );
    final scrollable = tester.state<ScrollableState>(
      find.ancestor(
        of: find.byType(FlarkSurface),
        matching: find.byType(Scrollable),
      ),
    );
    return (c, surface, scrollable.position);
  }

  Future<void> dispose(WidgetTester tester, FlarkController c) async {
    await tester.pump(const Duration(milliseconds: 350));
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  }

  testWidgets('a touch scroll moves neither the caret nor the keyboard', (
    tester,
  ) async {
    final (c, surface, position) = await pumpEditor(tester);
    final before = c.editor.selection;
    await tester.dragFrom(
      surface.localToGlobal(const Offset(120, 200)),
      const Offset(0, -240),
    );
    await tester.pump();
    expect(position.pixels, greaterThan(0));
    expect(c.editor.selection, before);
    expect(tester.testTextInput.hasAnyClients, isFalse);
    expect(tester.testTextInput.isVisible, isFalse);
    await dispose(tester, c);
  });

  testWidgets('a touch places the caret and raises the keyboard as it lifts', (
    tester,
  ) async {
    final (c, surface, _) = await pumpEditor(tester);
    final lineEnd = surface.localToGlobal(
      Offset(700, surface.caretRect.center.dy),
    );
    final gesture = await tester.startGesture(lineEnd);
    await tester.pump();
    expect(c.editor.selection, const FlarkSelection.collapsed(0));
    expect(tester.testTextInput.hasAnyClients, isFalse);
    await gesture.up();
    await tester.pump();
    expect(
      c.editor.selection,
      FlarkSelection.collapsed('Paragraph 0 has a few words.'.length),
    );
    expect(tester.testTextInput.isVisible, isTrue);
    await dispose(tester, c);
  });

  testWidgets('a tap raises a keyboard the platform hid', (tester) async {
    final (c, surface, _) = await pumpEditor(tester);
    final point = surface.localToGlobal(surface.caretRect.center);
    await tester.tapAt(point);
    await tester.pump();
    expect(tester.testTextInput.isVisible, isTrue);
    // Android's back gesture hides the keyboard and keeps the connection.
    await SystemChannels.textInput.invokeMethod<void>('TextInput.hide');
    expect(tester.testTextInput.isVisible, isFalse);
    await tester.pump(kDoubleTapTimeout);
    await tester.tapAt(point);
    await tester.pump();
    expect(tester.testTextInput.isVisible, isTrue);
    await dispose(tester, c);
  });

  testWidgets('the touch that stops a fling does not place the caret', (
    tester,
  ) async {
    final (c, surface, position) = await pumpEditor(tester);
    final before = c.editor.selection;
    await tester.flingFrom(
      surface.localToGlobal(const Offset(120, 300)),
      const Offset(0, -200),
      2000,
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 40));
    expect(position.isScrollingNotifier.value, isTrue);
    await tester.tapAt(tester.getCenter(find.byType(FlarkEditorWidget)));
    await tester.pump();
    expect(position.isScrollingNotifier.value, isFalse);
    expect(c.editor.selection, before);
    expect(tester.testTextInput.hasAnyClients, isFalse);
    await dispose(tester, c);
  });

  testWidgets('a touch double tap selects the word it lands on', (
    tester,
  ) async {
    final (c, surface, _) = await pumpEditor(
      tester,
      text: 'A simple word.',
      caret: 4,
    );
    final point = surface.localToGlobal(surface.caretRect.center);
    await tester.tapAt(point);
    await tester.pump(kDoubleTapMinTime);
    await tester.tapAt(point);
    await tester.pump();
    expect(c.editor.selection, const FlarkSelection(2, 8));
    await dispose(tester, c);
  });

  testWidgets('the reader opens a link on a tap, not on a scroll', (
    tester,
  ) async {
    final opened = <Uri>[];
    final (c, surface, position) = await pumpEditor(
      tester,
      text: '[guide](https://example.com/guide)\n\n$paragraphs',
      readOnly: true,
      onOpenLink: opened.add,
    );
    final link = surface.localToGlobal(
      surface.caretRect.center + const Offset(12, 0),
    );
    await tester.dragFrom(link, const Offset(0, -240));
    await tester.pumpAndSettle();
    expect(opened, isEmpty);
    position.jumpTo(0);
    await tester.pump();
    await tester.tapAt(link);
    expect(opened, [Uri.parse('https://example.com/guide')]);
    await dispose(tester, c);
  });
}
