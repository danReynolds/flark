import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// Up on the first line or Down on the last has nowhere to go, as Left at
/// the document's start or Right at its end: it is no press, so a pending
/// style and the typing's undo step outlast it.
void main() {
  final backend = createParseBackend();

  Future<FlarkController> mount(
    WidgetTester tester,
    String text,
    int caret,
  ) async {
    final c = FlarkController(FlarkEditor(backend, text: text, caret: caret));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c, autofocus: true)),
      ),
    );
    await tester.pump();
    return c;
  }

  for (final (name, key, text, caret) in [
    ('Up on the first line', LogicalKeyboardKey.arrowUp, 'one\n\ntwo', 3),
    ('Down on the last line', LogicalKeyboardKey.arrowDown, 'one\n\ntwo', 8),
    ('Left at the document start', LogicalKeyboardKey.arrowLeft, 'one', 0),
    ('Right at the document end', LogicalKeyboardKey.arrowRight, 'one', 3),
  ]) {
    testWidgets('a pending style outlasts $name', (tester) async {
      final c = await mount(tester, text, caret);
      expect(c.command(const ToggleStyle(Style.strong)), isTrue);
      final before = c.editor.selection;
      await tester.sendKeyEvent(key);
      await tester.pump();
      expect(c.editor.selection, before);
      c.command(const InsertText('x'));
      expect(c.text, contains('**x**'));
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    });
  }

  testWidgets('Up on the first line keeps the typing undo step', (
    tester,
  ) async {
    final c = await mount(tester, 'one', 3);
    c.command(const InsertText('a'));
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    c.command(const InsertText('b'));
    expect(c.text, 'oneab');
    c.command(const Undo());
    expect(c.text, 'one');
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
}
